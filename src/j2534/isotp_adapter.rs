//! SAE J2534 PassThru **ISO 15765** (native ISO-TP) transport.
//!
//! Opens a `PROTOCOL_ISO15765` channel so the adapter firmware handles all
//! ISO-TP framing, flow-control negotiation, and STmin timing in hardware.
//! The host only ever exchanges complete UDS PDUs.
//!
//! # Threading model
//!
//! Two dedicated background threads run concurrently:
//!
//! * **TX thread** — receives [`J2534IsoTpCmd::Send`] commands and calls
//!   `PassThruWriteMsgs` with a 60-second timeout.  This covers worst-case
//!   multi-frame transfers at 500 kbps with large ECU STmin values.
//! * **RX thread** — blocks on `PassThruReadMsgs` with a 500 ms fallback
//!   timeout and broadcasts complete UDS PDUs via a [`tokio::sync::broadcast`]
//!   channel.
//!
//! Both threads call the DLL concurrently on the same ISO 15765 channel.
//! Modern J2534 DLLs support concurrent `PassThruReadMsgs` /
//! `PassThruWriteMsgs` on the same channel; this is a documented precondition.
//!
//! On [`Drop`], `PassThruDisconnect` is called to interrupt any in-flight DLL
//! calls, then both threads are joined before `PassThruClose` releases the
//! device.  This avoids use-after-free in the DLL.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;

use async_stream::stream;
use tokio::sync::{broadcast, oneshot};

use crate::can::Identifier;
use crate::IsoTpTransport;

use super::common::{
    self, parse_can_id, FnPassThruDisconnect, FnPassThruReadMsgs, FnPassThruWriteMsgs, J2534Device,
    PassThruMsg,
};
use super::constants::{FilterType, IoctlId, IoctlParam, Protocol, Status};

/// `TxFlags` flag: pad outbound CAN frames to DLC = 8.
const ISO15765_FRAME_PAD: u32 = 0x0040;

/// Encode a separation-time value in microseconds to the ISO 15765-2 STmin byte.
///
/// Encoding (ISO 15765-2 §9.6.5.4 / Table 5):
/// - 0 µs → `0x00` (no delay)
/// - 1 000–127 000 µs (1–127 ms, step 1 ms) → `0x01`–`0x7F`
/// - 100–900 µs (step 100 µs) → `0xF1`–`0xF9`
/// - Values below 100 µs (other than 0) or above 127 ms → nearest boundary.
pub fn us_to_stmin_byte(us: u32) -> u8 {
    if us == 0 {
        0x00
    } else if us < 1_000 {
        let steps = us / 100;
        if steps == 0 {
            0x00
        } else {
            0xF0 + steps.min(9) as u8
        }
    } else {
        let ms = us / 1_000;
        ms.min(127) as u8
    }
}

enum J2534IsoTpCmd {
    Send(Vec<u8>, oneshot::Sender<Result<(), String>>),
}

#[derive(Clone)]
enum J2534IsoTpEvt {
    Pdu(Vec<u8>),
    Disconnected,
}

/// J2534 ISO 15765 (native ISO-TP) transport.
///
/// Implements [`IsoTpTransport`] so it plugs directly into `UDSClient`
/// without going through the software ISO-TP layer.
pub struct J2534NativeIsoTpTransport {
    tx_cmd: Option<SyncSender<J2534IsoTpCmd>>,
    rx_bcast: broadcast::Sender<J2534IsoTpEvt>,
    stop_rx: Arc<AtomicBool>,
    tx_thread: Option<thread::JoinHandle<()>>,
    rx_thread: Option<thread::JoinHandle<()>>,
    channel_id: u32,
    pass_thru_disconnect: FnPassThruDisconnect,
    device: Option<J2534Device>,
}

impl J2534NativeIsoTpTransport {
    /// Open a J2534 ISO 15765 channel and start the TX/RX background threads.
    ///
    /// Opens a new device via `PassThruOpen`.  To reuse an already-open
    /// device (e.g. after an OBD DTC-clear channel), use
    /// [`open_on_device`](Self::open_on_device) instead.
    pub fn open(
        dll_path: Option<&str>,
        bitrate: u32,
        tx_id: Identifier,
        rx_id: Identifier,
        stmin_tx_us: Option<u32>,
    ) -> Result<Self, String> {
        let device = common::open_device(dll_path)?;
        Self::open_on_device(device, bitrate, tx_id, rx_id, stmin_tx_us)
            .map_err(|(msg, _device)| msg)
    }

    /// Open an ISO 15765 channel on an already-open [`J2534Device`].
    ///
    /// This avoids closing and reopening the physical device when switching
    /// channels (e.g. from OBD DTC-clear to the main flash channel).
    ///
    /// On error, the [`J2534Device`] is returned alongside the error message
    /// so the caller can reuse it.
    pub fn open_on_device(
        device: J2534Device,
        bitrate: u32,
        tx_id: Identifier,
        rx_id: Identifier,
        stmin_tx_us: Option<u32>,
    ) -> Result<Self, (String, J2534Device)> {
        let device_id = device.device_id;
        let pass_thru_connect = device.connect;
        let pass_thru_disconnect = device.disconnect;
        let pass_thru_read = device.read;
        let pass_thru_write = device.write;
        let pass_thru_filter = device.filter;
        let pass_thru_ioctl = device.ioctl;

        // Connect using the ISO15765 protocol specifier
        let mut channel_id: u32 = 0;
        let status = Status::from(unsafe {
            pass_thru_connect(
                device_id,
                Protocol::Iso15765.into(),
                0,
                bitrate,
                &mut channel_id,
            )
        });
        tracing::debug!(ret = %status, channel_id, bitrate, "PassThruConnect ISO15765");
        if status != Status::NoError {
            return Err((
                format!("PassThruConnect (ISO15765, {bitrate} bps) failed: {status}"),
                device,
            ));
        }

        let proto: u32 = Protocol::Iso15765.into();
        let mut mask_msg = PassThruMsg::new_raw(proto, 0xFFFF_FFFF, &[]);
        let mut pattern_msg = PassThruMsg::new(proto, rx_id, &[]);
        let mut fc_msg = PassThruMsg::new(proto, tx_id, &[]);
        mask_msg.tx_flags = ISO15765_FRAME_PAD;
        pattern_msg.tx_flags = ISO15765_FRAME_PAD;
        fc_msg.tx_flags = ISO15765_FRAME_PAD;

        let mut filter_id: u32 = 0;
        let status = Status::from(unsafe {
            pass_thru_filter(
                channel_id,
                FilterType::FlowControl.into(),
                &mask_msg,
                &pattern_msg,
                &fc_msg,
                &mut filter_id,
            )
        });
        let tx_raw: u32 = tx_id.into();
        let rx_raw: u32 = rx_id.into();
        tracing::debug!(
            ret = %status,
            filter_id,
            tx_id = format_args!("{tx_raw:08X}"),
            rx_id = format_args!("{rx_raw:08X}"),
            "PassThruStartMsgFilter (FLOW_CONTROL)"
        );
        if status != Status::NoError {
            unsafe { pass_thru_disconnect(channel_id) };
            return Err((
                format!("PassThruStartMsgFilter failed: {status}"),
                device,
            ));
        }

        // Clear receive buffer to ensure filter is applied correctly
        let status = Status::from(unsafe {
            pass_thru_ioctl(
                channel_id,
                IoctlId::ClearRxBuffer.into(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        });
        tracing::debug!(ret = %status, "PassThruIoctl CLEAR_RX_BUFFER");

        // Set receive ISO15765_STMIN = 0 to get fastest allowed by the module
        let status = common::set_config(pass_thru_ioctl, channel_id, IoctlParam::Iso15765Stmin, 0);
        tracing::debug!(ret = %status, "PassThruIoctl SET_CONFIG ISO15765_STMIN=0");

        // STMIN_TX ioctl; if stmin_tx is not specified, do not invoke ioctl
        // (which allows the adapter to choose based on the control flow frames)
        if let Some(stmin_us) = stmin_tx_us {
            let stmin_byte = us_to_stmin_byte(stmin_us) as u32;
            let status =
                common::set_config(pass_thru_ioctl, channel_id, IoctlParam::StminTx, stmin_byte);
            tracing::debug!(ret = %status, stmin_us, stmin_byte, "PassThruIoctl SET_CONFIG STMIN_TX");
            if status != Status::NoError {
                tracing::warn!(
                    "STMIN_TX ioctl failed: {status} — \
                    adapter will use its default separation time"
                );
            }
        }

        // Create channels and spawn threads
        let (tx_cmd, rx_cmd) = mpsc::sync_channel::<J2534IsoTpCmd>(64);
        let (bcast_tx, bcast_rx) = broadcast::channel::<J2534IsoTpEvt>(256);
        let stop_rx = Arc::new(AtomicBool::new(false));

        // Drop the initial receiver; callers subscribe via bcast_tx.
        drop(bcast_rx);

        let tx_thread = {
            let bcast = bcast_tx.clone();
            let stop = stop_rx.clone();
            thread::Builder::new()
                .name("j2534-isotp-tx".to_owned())
                .spawn(move || {
                    isotp_tx_thread(channel_id, tx_id, pass_thru_write, rx_cmd, bcast, stop)
                })
                .map_err(|e| format!("Failed to spawn J2534 ISO-TP TX thread: {e}"))
        };
        let tx_thread = match tx_thread {
            Ok(h) => h,
            Err(e) => return Err((e, device)),
        };

        let rx_thread = {
            let bcast = bcast_tx.clone();
            let stop = stop_rx.clone();
            thread::Builder::new()
                .name("j2534-isotp-rx".to_owned())
                .spawn(move || isotp_rx_thread(channel_id, pass_thru_read, bcast, stop))
                .map_err(|e| format!("Failed to spawn J2534 ISO-TP RX thread: {e}"))
        };
        let rx_thread = match rx_thread {
            Ok(h) => h,
            Err(e) => return Err((e, device)),
        };

        Ok(Self {
            tx_cmd: Some(tx_cmd),
            rx_bcast: bcast_tx,
            stop_rx,
            tx_thread: Some(tx_thread),
            rx_thread: Some(rx_thread),
            channel_id,
            pass_thru_disconnect,
            device: Some(device),
        })
    }

    /// Disconnect the ISO 15765 channel and return the underlying
    /// [`J2534Device`] so it can be reused for another channel.
    pub fn into_device(mut self) -> J2534Device {
        self.shutdown_channel();
        self.device.take().expect("device already taken")
    }

    fn shutdown_channel(&mut self) {
        drop(self.tx_cmd.take());
        self.stop_rx.store(true, Ordering::Release);
        let status = Status::from(unsafe { (self.pass_thru_disconnect)(self.channel_id) });
        tracing::trace!(ret = %status, "PassThruDisconnect");
        if let Some(h) = self.tx_thread.take() {
            let _ = h.join();
        }
        if let Some(h) = self.rx_thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for J2534NativeIsoTpTransport {
    fn drop(&mut self) {
        self.shutdown_channel();
        drop(self.device.take());
    }
}

impl IsoTpTransport for J2534NativeIsoTpTransport {
    fn send<'a>(
        &'a self,
        data: &'a [u8],
    ) -> impl std::future::Future<Output = crate::Result<()>> + 'a {
        let pdu = data.to_vec();
        async move {
            let Some(tx) = &self.tx_cmd else {
                return Err(crate::Error::Disconnected);
            };
            let (done_tx, done_rx) = oneshot::channel();
            tx.send(J2534IsoTpCmd::Send(pdu, done_tx))
                .map_err(|_| crate::Error::Disconnected)?;
            done_rx
                .await
                .map_err(|_| crate::Error::Disconnected)?
                .map_err(|_| crate::Error::Disconnected)
        }
    }

    fn recv(&self) -> impl crate::Stream<Item = crate::Result<Vec<u8>>> + Unpin + '_ {
        let mut rx = self.rx_bcast.subscribe();
        Box::pin(stream! {
            loop {
                match rx.recv().await {
                    Ok(J2534IsoTpEvt::Pdu(pdu)) => yield Ok(pdu),
                    Ok(J2534IsoTpEvt::Disconnected) => {
                        yield Err(crate::Error::Disconnected);
                        return;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        yield Err(crate::Error::Disconnected);
                        return;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            dropped = n,
                            "J2534 ISO15765 RX lagged — PDU(s) dropped"
                        );
                    }
                }
            }
        })
    }
}

fn isotp_tx_thread(
    channel_id: u32,
    tx_id: Identifier,
    write: FnPassThruWriteMsgs,
    rx_cmds: Receiver<J2534IsoTpCmd>,
    _bcast: broadcast::Sender<J2534IsoTpEvt>,
    stop_rx: Arc<AtomicBool>,
) {
    while let Ok(J2534IsoTpCmd::Send(pdu, done)) = rx_cmds.recv() {
        tracing::debug!(
            len = pdu.len(),
            payload = %hex::encode(&pdu[..pdu.len().min(16)]),
            "J2534 ISO15765 TX"
        );

        let mut msg = PassThruMsg::new(Protocol::Iso15765.into(), tx_id, &pdu);
        msg.tx_flags = ISO15765_FRAME_PAD;

        let mut count: u32 = 1;

        let status = Status::from(unsafe { write(channel_id, &mut msg, &mut count, 60_000) });
        tracing::debug!(ret = %status, "PassThruWriteMsgs ISO15765");

        let result = if status == Status::NoError {
            Ok(())
        } else {
            Err(format!("ISO15765 TX failed: {status}"))
        };
        done.send(result).ok();
    }
    stop_rx.store(true, Ordering::Release);
}

fn isotp_rx_thread(
    channel_id: u32,
    read: FnPassThruReadMsgs,
    bcast: broadcast::Sender<J2534IsoTpEvt>,
    stop: Arc<AtomicBool>,
) {
    let mut msg = PassThruMsg::default();

    loop {
        let mut count: u32 = 1;
        let status = Status::from(unsafe { read(channel_id, &mut msg, &mut count, 500) });

        match status {
            Status::NoError if count > 0 => {
                let len = msg.data_size as usize;
                if len < 4 {
                    continue;
                }

                let src_id = parse_can_id(&msg.data);
                let src_raw: u32 = src_id.into();
                let payload = &msg.data[4..len];

                if msg.rx_status != 0 {
                    tracing::debug!(
                        rx_status = format_args!("0x{:04X}", msg.rx_status),
                        src_id = format_args!("{src_raw:08X}"),
                        data_size = payload.len(),
                        payload = %hex::encode(&payload[..payload.len().min(16)]),
                        "J2534 ISO15765 skipping non-data frame"
                    );
                    continue;
                }

                let pdu = payload.to_vec();
                tracing::debug!(
                    src_id = format_args!("{src_raw:08X}"),
                    len = pdu.len(),
                    payload = %hex::encode(&pdu[..pdu.len().min(16)]),
                    "J2534 ISO15765 RX"
                );

                bcast.send(J2534IsoTpEvt::Pdu(pdu)).ok();
            }
            Status::Timeout | Status::BufferEmpty | Status::NoError => {
                if stop.load(Ordering::Acquire) {
                    return;
                }
            }
            _ => {
                tracing::debug!(
                    ret = %status,
                    "J2534 ISO15765 RX error — channel disconnected, exiting"
                );
                bcast.send(J2534IsoTpEvt::Disconnected).ok();
                return;
            }
        }
    }
}
