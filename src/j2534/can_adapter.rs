//! SAE J2534 PassThru CAN adapter.
//!
//! Two dedicated background threads — one for transmit, one for receive —
//! call `PassThruWriteMsgs` and `PassThruReadMsgs` concurrently on the same
//! channel.  Modern J2534 DLLs support concurrent read/write on the same
//! channel; this is documented as a precondition for using this adapter.
//!
//! On [`Drop`], `PassThruDisconnect` is called first to interrupt any
//! in-flight DLL calls, then both threads are joined before `PassThruClose`
//! releases the device.  This avoids use-after-free in the DLL.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;
use std::thread;

use tokio::sync::broadcast;

use crate::can::{CanAdapter, Frame, Identifier};

use super::common::{
    self, FnPassThruDisconnect, FnPassThruReadMsgs, FnPassThruWriteMsgs,
    J2534Device, PassThruMsg, STATUS_NOERROR, ERR_BUFFER_EMPTY, ERR_TIMEOUT,
    parse_can_id,
};

// Protocol / filter constants

const PROTOCOL_CAN: u32 = 5;
const FILTER_PASS: u32 = 1;

// Internal channel types
enum J2534Cmd {
    Send { id: Identifier, data: Vec<u8> },
}

#[derive(Clone)]
enum J2534CanEvt {
    Frame { id: Identifier, data: Vec<u8>, loopback: bool },
    Disconnected,
}

// Public adapter struct

/// CAN adapter backed by a SAE J2534 PassThru device.
///
/// Two dedicated background threads handle transmit and receive concurrently.
/// The struct is [`Send`] (not [`Sync`]) so it can be moved into the
/// [`crate::can::AsyncCanAdapter`] processing thread.
///
/// Loopback frames are synthesised in software after each successful
/// `PassThruWriteMsgs` call.  Hardware loopback (`SET_CONFIG(LOOPBACK=1)`)
/// is intentionally avoided because many target devices do not support it.
pub struct J2534CanAdapter {
    /// Commands to the TX thread.
    tx_cmd: Option<SyncSender<J2534Cmd>>,
    /// Subscription to the RX broadcast channel.  Stored directly (not in a
    /// `Mutex`) because `CanAdapter::recv` takes `&mut self`.
    rx_sub: broadcast::Receiver<J2534CanEvt>,
    /// Signals the RX thread to exit.
    stop_rx: Arc<AtomicBool>,
    tx_thread: Option<thread::JoinHandle<()>>,
    rx_thread: Option<thread::JoinHandle<()>>,
    channel_id: u32,
    pass_thru_disconnect: FnPassThruDisconnect,
    /// The underlying device handle; taken by `into_device()`.
    device: Option<J2534Device>,
}

impl J2534CanAdapter {
    /// Open a J2534 CAN channel and start the TX/RX background threads.
    ///
    /// Opens a new device via [`open_device`](super::open_device).  To reuse
    /// an already-open device, use [`open_on_device`](Self::open_on_device).
    ///
    /// * `dll_path` — path to the PassThru DLL, or `None` to auto-discover
    ///   the first 64-bit driver from the Windows registry.
    /// * `bitrate` — CAN bitrate in bits/sec (e.g. `500_000`).
    pub fn open(dll_path: Option<&str>, bitrate: u32) -> Result<Self, String> {
        let device = common::open_device(dll_path)?;
        Self::open_on_device(device, bitrate)
            .map_err(|(msg, _device)| msg)
    }

    /// Open a CAN channel on an already-open [`J2534Device`].
    ///
    /// This avoids closing and reopening the physical device when switching
    /// channels (e.g. reusing the same adapter for CAN after an ISO 15765
    /// channel was closed).
    ///
    /// On error, the [`J2534Device`] is returned alongside the error message
    /// so the caller can reuse it.
    pub fn open_on_device(
        device: J2534Device,
        bitrate: u32,
    ) -> Result<Self, (String, J2534Device)> {
        let device_id = device.device_id;
        let pass_thru_connect = device.connect;
        let pass_thru_disconnect = device.disconnect;
        let pass_thru_read = device.read;
        let pass_thru_write = device.write;
        let pass_thru_filter = device.filter;

        // Open CAN channel
        let mut channel_id: u32 = 0;
        let ret = unsafe {
            pass_thru_connect(device_id, PROTOCOL_CAN, 0, bitrate, &mut channel_id)
        };
        tracing::debug!(ret = common::status_str(ret), channel_id, bitrate, "PassThruConnect CAN");
        if ret != STATUS_NOERROR {
            return Err((format!(
                "PassThruConnect (CAN, {bitrate} bps) failed: 0x{ret:02X} ({})",
                common::status_str(ret)
            ), device));
        }

        // Install pass-all receive filter
        // Mask and pattern both all-zero: every frame passes regardless of ID.
        let zero_msg = PassThruMsg::new_raw(PROTOCOL_CAN, 0, &[]);
        let mut filter_id: u32 = 0;
        let ret = unsafe {
            pass_thru_filter(
                channel_id,
                FILTER_PASS,
                &zero_msg,
                &zero_msg,
                std::ptr::null(),
                &mut filter_id,
            )
        };
        tracing::debug!(ret = common::status_str(ret), filter_id, "PassThruStartMsgFilter");
        if ret != STATUS_NOERROR {
            unsafe { pass_thru_disconnect(channel_id) };
            return Err((format!(
                "PassThruStartMsgFilter (PASS, pass-all) failed: 0x{ret:02X} ({})",
                common::status_str(ret)
            ), device));
        }

        // Create channels and spawn threads
        let (tx_cmd, rx_cmd) = mpsc::sync_channel::<J2534Cmd>(64);
        let (bcast_tx, bcast_rx) = broadcast::channel::<J2534CanEvt>(1024);
        let stop_rx = Arc::new(AtomicBool::new(false));

        let tx_thread = {
            let bcast = bcast_tx.clone();
            let stop = stop_rx.clone();
            thread::Builder::new()
                .name("j2534-can-tx".to_owned())
                .spawn(move || can_tx_thread(channel_id, pass_thru_write, rx_cmd, bcast, stop))
                .map_err(|e| format!("Failed to spawn J2534 CAN TX thread: {e}"))
        };
        let tx_thread = match tx_thread {
            Ok(h) => h,
            Err(e) => return Err((e, device)),
        };

        let rx_thread = {
            let stop = stop_rx.clone();
            thread::Builder::new()
                .name("j2534-can-rx".to_owned())
                .spawn(move || can_rx_thread(channel_id, pass_thru_read, bcast_tx, stop))
                .map_err(|e| format!("Failed to spawn J2534 CAN RX thread: {e}"))
        };
        let rx_thread = match rx_thread {
            Ok(h) => h,
            Err(e) => return Err((e, device)),
        };

        Ok(Self {
            tx_cmd: Some(tx_cmd),
            rx_sub: bcast_rx,
            stop_rx,
            tx_thread: Some(tx_thread),
            rx_thread: Some(rx_thread),
            channel_id,
            pass_thru_disconnect,
            device: Some(device),
        })
    }

    /// Disconnect the CAN channel and return the underlying [`J2534Device`]
    /// so it can be reused for another channel.
    pub fn into_device(mut self) -> J2534Device {
        self.shutdown_channel();
        self.device.take().expect("device already taken")
    }

    fn shutdown_channel(&mut self) {
        // Signal TX thread to stop (its recv() will return Err).
        drop(self.tx_cmd.take());
        // Signal RX thread to stop.
        self.stop_rx.store(true, Ordering::Release);
        // Disconnect invalidates the channel, causing in-flight
        // PassThruReadMsgs / PassThruWriteMsgs to return an error.
        let ret = unsafe { (self.pass_thru_disconnect)(self.channel_id) };
        tracing::trace!(ret = common::status_str(ret), "PassThruDisconnect");
        // Join threads BEFORE PassThruClose — the threads may still be inside
        // a DLL call that references device-level structures.  Closing the
        // device while a read/write is in-flight causes a use-after-free.
        if let Some(h) = self.tx_thread.take() {
            let _ = h.join();
        }
        if let Some(h) = self.rx_thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for J2534CanAdapter {
    fn drop(&mut self) {
        self.shutdown_channel();
        // Drop the device (calls PassThruClose).
        drop(self.device.take());
    }
}

impl CanAdapter for J2534CanAdapter {
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> crate::Result<()> {
        let Some(tx) = &self.tx_cmd else {
            return Ok(());
        };
        while let Some(frame) = frames.pop_front() {
            match tx.try_send(J2534Cmd::Send { id: frame.id, data: frame.data.clone() }) {
                Ok(()) => {}
                Err(_) => {
                    // TX queue full — restore frame and retry next cycle.
                    frames.push_front(frame);
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn recv(&mut self) -> crate::Result<Vec<Frame>> {
        let mut frames = Vec::new();
        loop {
            match self.rx_sub.try_recv() {
                Ok(J2534CanEvt::Frame { id, data, loopback }) => {
                    if let Ok(mut frame) = Frame::new(0, id, &data) {
                        frame.loopback = loopback;
                        frames.push(frame);
                    }
                }
                Ok(J2534CanEvt::Disconnected) => return Err(crate::Error::Disconnected),
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Closed) => {
                    return Err(crate::Error::Disconnected)
                }
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!(dropped = n, "J2534 CAN RX broadcast lagged — frames dropped");
                }
            }
        }
        Ok(frames)
    }
}

// TX background thread

/// Transmit thread: dequeues [`J2534Cmd`] items and writes them to the CAN
/// channel.  Synthesises a software loopback frame after each successful send.
fn can_tx_thread(
    channel_id: u32,
    write: FnPassThruWriteMsgs,
    rx_cmds: std::sync::mpsc::Receiver<J2534Cmd>,
    bcast: broadcast::Sender<J2534CanEvt>,
    stop_rx: Arc<AtomicBool>,
) {
    while let Ok(J2534Cmd::Send { id, data }) = rx_cmds.recv() {
        let arb_id: u32 = id.into();
        tracing::debug!(
            id = format_args!("{arb_id:08X}"),
            payload = %hex::encode(&data),
            "J2534 TX"
        );
        let mut msg = PassThruMsg::new(PROTOCOL_CAN, id, &data);
        let mut count: u32 = 1;
        // 100 ms timeout: short enough that Drop's PassThruDisconnect
        // will interrupt us promptly.
        let ret = unsafe { write(channel_id, &mut msg, &mut count, 100) };
        tracing::trace!(ret = common::status_str(ret), count, "PassThruWriteMsgs");
        if ret == STATUS_NOERROR {
            // Software loopback: hardware loopback is unreliable on many adapters.
            bcast.send(J2534CanEvt::Frame { id, data, loopback: true }).ok();
        } else {
            tracing::debug!(
                ret = common::status_str(ret),
                "J2534 TX error (channel may be disconnected)"
            );
        }
    }
    // Tell the RX thread to stop.
    stop_rx.store(true, Ordering::Release);
}

// RX background thread

/// Receive thread: blocks on `PassThruReadMsgs` waiting for one CAN frame at
/// a time and broadcasts each frame.
///
/// Using count=1 with a long blocking timeout means the thread sleeps
/// efficiently in the DLL when the bus is quiet, rather than spinning.
/// `Drop` calls `PassThruDisconnect` which interrupts any blocked read
/// immediately.  The 500 ms fallback timeout handles DLLs that do not
/// properly interrupt on disconnect.
fn can_rx_thread(
    channel_id: u32,
    read: FnPassThruReadMsgs,
    bcast: broadcast::Sender<J2534CanEvt>,
    stop: Arc<AtomicBool>,
) {
    let mut msg = PassThruMsg::default();

    loop {
        let mut count: u32 = 1;
        let ret = unsafe { read(channel_id, &mut msg, &mut count, 500) };

        match ret {
            STATUS_NOERROR if count > 0 => {
                let len = msg.data_size as usize;
                if len < 4 {
                    tracing::trace!(
                        rx_status = format_args!("0x{:04X}", msg.rx_status),
                        data_size = msg.data_size,
                        "J2534 RX skipped (frame too short)"
                    );
                } else {
                    let id = parse_can_id(&msg.data);
                    let data = msg.data[4..len].to_vec();
                    let arb_id: u32 = id.into();
                    tracing::debug!(
                        id = format_args!("{arb_id:08X}"),
                        payload = %hex::encode(&data),
                        "J2534 RX"
                    );
                    bcast.send(J2534CanEvt::Frame { id, data, loopback: false }).ok();
                }
            }
            ERR_TIMEOUT | ERR_BUFFER_EMPTY | STATUS_NOERROR => {
                if stop.load(Ordering::Acquire) {
                    return;
                }
            }
            _ => {
                tracing::debug!(
                    ret = common::status_str(ret),
                    "J2534 RX error — channel disconnected, exiting"
                );
                bcast.send(J2534CanEvt::Disconnected).ok();
                return;
            }
        }
    }
}
