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
use crate::isotp::{duration_to_stmin_byte, IsoTPConfig};
use crate::{IsoTpTransport, Result};

use super::common::{self, parse_can_id, J2534Channel, J2534Device, PassThruMsg};
use super::constants::{
    IoctlParam, Protocol, Status, CAN_29BIT_ID_FLAG, ISO15765_ADDR_TYPE, ISO15765_FRAME_PAD,
    ISO15765_PADDING_ERROR,
};
use super::error::Error as J2534Error;

/// Timeout for blocking ISO 15765 TX writes.
const ISO15765_TX_TIMEOUT_MS: u32 = 60_000;

/// Timeout for blocking ISO 15765 RX reads.
const ISO15765_RX_TIMEOUT_MS: u32 = 500;

enum J2534IsoTpCmd {
    Send(Vec<u8>, oneshot::Sender<Result<()>>),
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
    channel: Option<J2534Channel>,
    device: Option<J2534Device>,
}

impl J2534NativeIsoTpTransport {
    /// Open a J2534 ISO 15765 channel and start the TX/RX background threads.
    ///
    /// Opens a new device via `PassThruOpen`.  To reuse an already-open
    /// device (e.g. after an OBD DTC-clear channel), use
    /// [`open_on_device`](Self::open_on_device) instead.
    pub fn open(dll_path: Option<&str>, bitrate: u32, config: IsoTPConfig) -> Result<Self> {
        let device = common::open_device(dll_path)?;
        Self::open_on_device(device, bitrate, config)
    }

    /// Open an ISO 15765 channel on an already-open [`J2534Device`].
    ///
    /// This avoids closing and reopening the physical device when switching
    /// channels (e.g. from OBD DTC-clear to the main flash channel).
    /// Uses `tx_id`, `rx_id`, `padding`, `separation_time_min`, and
    /// `ext_address` from `config`.
    /// The remaining [`IsoTPConfig`] fields are ignored by the native J2534
    /// transport.
    ///
    pub fn open_on_device(device: J2534Device, bitrate: u32, config: IsoTPConfig) -> Result<Self> {
        let tx_id = config.tx_id;
        let rx_id = config.rx_id;
        let ext_address = config.ext_address;
        let tx_flags = isotp_tx_flags(config.padding, ext_address);
        let connect_flags = isotp_connect_flags(ext_address);
        let stmin_tx = config.separation_time_min;

        let channel = common::connect_channel_with_flags(
            &device,
            Protocol::Iso15765,
            connect_flags,
            bitrate,
        )?;

        let status =
            channel.install_iso15765_flow_control_filter(tx_id, rx_id, ext_address, tx_flags);
        if status != Status::NoError {
            let _ = channel.disconnect();
            return Err(
                J2534Error::DllError(format!("PassThruStartMsgFilter failed: {status}")).into(),
            );
        }

        // Clear receive buffer to ensure filter is applied correctly
        let status = channel.clear_rx_buffer();
        tracing::debug!(ret = %status, "PassThruIoctl CLEAR_RX_BUFFER");

        // Set receive ISO15765_STMIN = 0 to get fastest allowed by the module
        let status = channel.set_config(IoctlParam::Iso15765Stmin, 0);
        tracing::debug!(ret = %status, "PassThruIoctl SET_CONFIG ISO15765_STMIN=0");

        // STMIN_TX ioctl; if stmin_tx is not specified, do not invoke ioctl
        // (which allows the adapter to choose based on the control flow frames)
        if let Some(stmin_tx) = stmin_tx {
            let stmin_byte = duration_to_stmin_byte(stmin_tx) as u32;
            let status = channel.set_config(IoctlParam::StminTx, stmin_byte);
            tracing::debug!(ret = %status, stmin = ?stmin_tx, stmin_byte, "PassThruIoctl SET_CONFIG STMIN_TX");
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
                    isotp_tx_thread(channel, tx_id, ext_address, tx_flags, rx_cmd, bcast, stop)
                })
                .map_err(|e| {
                    J2534Error::DllError(format!("Failed to spawn J2534 ISO-TP TX thread: {e}"))
                })
        }?;

        let rx_thread = {
            let bcast = bcast_tx.clone();
            let stop = stop_rx.clone();
            thread::Builder::new()
                .name("j2534-isotp-rx".to_owned())
                .spawn(move || isotp_rx_thread(channel, ext_address, bcast, stop))
                .map_err(|e| {
                    J2534Error::DllError(format!("Failed to spawn J2534 ISO-TP RX thread: {e}"))
                })
        }?;

        Ok(Self {
            tx_cmd: Some(tx_cmd),
            rx_bcast: bcast_tx,
            stop_rx,
            tx_thread: Some(tx_thread),
            rx_thread: Some(rx_thread),
            channel: Some(channel),
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
        if let Some(channel) = self.channel.take() {
            let status = channel.disconnect();
            tracing::trace!(ret = %status, "PassThruDisconnect");
        }
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
            done_rx.await.map_err(|_| crate::Error::Disconnected)?
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

fn isotp_connect_flags(ext_address: Option<u8>) -> u32 {
    if ext_address.is_some() {
        ISO15765_ADDR_TYPE
    } else {
        0
    }
}

fn isotp_tx_flags(padding: Option<u8>, ext_address: Option<u8>) -> u32 {
    let mut flags = 0;
    if padding.is_some() {
        flags |= ISO15765_FRAME_PAD;
    }
    if ext_address.is_some() {
        flags |= ISO15765_ADDR_TYPE;
    }
    flags
}

fn isotp_tx_thread(
    channel: J2534Channel,
    tx_id: Identifier,
    ext_address: Option<u8>,
    tx_flags: u32,
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

        let mut msg =
            PassThruMsg::new_with_ext_address(Protocol::Iso15765.into(), tx_id, ext_address, &pdu);
        msg.tx_flags = tx_flags;

        let (status, count) = channel.write_message(&mut msg, ISO15765_TX_TIMEOUT_MS);
        tracing::debug!(ret = %status, count, "PassThruWriteMsgs ISO15765");

        let result = if status == Status::NoError {
            Ok(())
        } else {
            Err(J2534Error::DllError(format!("ISO15765 TX failed: {status}")).into())
        };
        done.send(result).ok();
    }
    stop_rx.store(true, Ordering::Release);
}

fn isotp_rx_thread(
    channel: J2534Channel,
    ext_address: Option<u8>,
    bcast: broadcast::Sender<J2534IsoTpEvt>,
    stop: Arc<AtomicBool>,
) {
    let mut msg = PassThruMsg::default();

    loop {
        let (status, count) = channel.read_message(&mut msg, ISO15765_RX_TIMEOUT_MS);

        match status {
            Status::NoError if count > 0 => {
                let len = msg.data_size as usize;
                let header_len = 4 + usize::from(ext_address.is_some());
                if len < header_len {
                    continue;
                }

                let src_id = parse_can_id(&msg.data);
                let src_raw: u32 = src_id.into();
                if let Some(ext_address) = ext_address {
                    let actual_ext_address = msg.data[4];
                    if actual_ext_address != ext_address {
                        tracing::debug!(
                            expected_ext_address = format_args!("0x{ext_address:02X}"),
                            actual_ext_address = format_args!("0x{actual_ext_address:02X}"),
                            src_id = format_args!("{src_raw:08X}"),
                            "J2534 ISO15765 skipping frame with mismatched extended address"
                        );
                        continue;
                    }
                }
                let payload = &msg.data[header_len..len];
                let informational_rx_status = msg.rx_status
                    & (ISO15765_ADDR_TYPE | ISO15765_PADDING_ERROR | CAN_29BIT_ID_FLAG);
                let non_data_rx_status = msg.rx_status & !informational_rx_status;

                if non_data_rx_status != 0 {
                    tracing::debug!(
                        rx_status = format_args!("0x{:04X}", msg.rx_status),
                        src_id = format_args!("{src_raw:08X}"),
                        data_size = payload.len(),
                        payload = %hex::encode(&payload[..payload.len().min(16)]),
                        "J2534 ISO15765 skipping non-data frame"
                    );
                    continue;
                }
                if informational_rx_status & ISO15765_PADDING_ERROR != 0 {
                    tracing::warn!(
                        rx_status = format_args!("0x{:04X}", msg.rx_status),
                        src_id = format_args!("{src_raw:08X}"),
                        "J2534 ISO15765 RX padding error"
                    );
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
