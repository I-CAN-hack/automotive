//! SAE J2534 PassThru CAN adapter.
//!
//! Implements [`CanAdapter`] directly on top of a J2534 CAN channel and relies
//! on [`crate::can::AsyncCanAdapter`] for background polling, retry logic, and
//! async send/receive orchestration.

use std::collections::VecDeque;

use crate::can::{CanAdapter, Frame};

use super::common::{
    self, parse_can_id, FnPassThruDisconnect, FnPassThruReadMsgs, FnPassThruWriteMsgs, J2534Device,
    PassThruMsg,
};
use super::constants::{FilterType, Protocol, Status};

/// Poll the J2534 channel without blocking the `AsyncCanAdapter` process loop.
const IO_TIMEOUT_MS: u32 = 0;

/// CAN adapter backed by a SAE J2534 PassThru device.
///
/// The adapter performs non-blocking `PassThruReadMsgs` / `PassThruWriteMsgs`
/// calls from the existing `AsyncCanAdapter` background thread. Successful
/// writes enqueue a synthetic loopback frame, matching the behaviour expected
/// by the generic async wrapper.
pub struct J2534CanAdapter {
    loopback_queue: VecDeque<Frame>,
    channel_id: u32,
    connected: bool,
    read: FnPassThruReadMsgs,
    write: FnPassThruWriteMsgs,
    pass_thru_disconnect: FnPassThruDisconnect,
    device: Option<J2534Device>,
}

impl J2534CanAdapter {
    /// Open a J2534 CAN channel.
    ///
    /// Opens a new device via [`open_device`](super::open_device).  To reuse
    /// an already-open device, use [`open_on_device`](Self::open_on_device).
    ///
    /// * `dll_path` — path to the PassThru DLL, or `None` to auto-discover
    ///   the first 64-bit driver from the Windows registry.
    /// * `bitrate` — CAN bitrate in bits/sec (e.g. `500_000`).
    pub fn open(dll_path: Option<&str>, bitrate: u32) -> Result<Self, String> {
        let device = common::open_device(dll_path)?;
        Self::open_on_device(device, bitrate).map_err(|(msg, _device)| msg)
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
        let status = Status::from(unsafe {
            pass_thru_connect(device_id, Protocol::Can.into(), 0, bitrate, &mut channel_id)
        });
        tracing::debug!(ret = %status, channel_id, bitrate, "PassThruConnect CAN");
        if status != Status::NoError {
            return Err((
                format!("PassThruConnect (CAN, {bitrate} bps) failed: {status}"),
                device,
            ));
        }

        // Install pass-all receive filter
        // Mask and pattern both all-zero: every frame passes regardless of ID.
        let zero_msg = PassThruMsg::new_raw(Protocol::Can.into(), 0, &[]);
        let mut filter_id: u32 = 0;
        let status = Status::from(unsafe {
            pass_thru_filter(
                channel_id,
                FilterType::Pass.into(),
                &zero_msg,
                &zero_msg,
                std::ptr::null(),
                &mut filter_id,
            )
        });
        tracing::debug!(ret = %status, filter_id, "PassThruStartMsgFilter");
        if status != Status::NoError {
            unsafe { pass_thru_disconnect(channel_id) };
            return Err((
                format!("PassThruStartMsgFilter (PASS, pass-all) failed: {status}"),
                device,
            ));
        }

        Ok(Self {
            loopback_queue: VecDeque::new(),
            channel_id,
            connected: true,
            read: pass_thru_read,
            write: pass_thru_write,
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
        if !self.connected {
            return;
        }
        self.connected = false;

        // Disconnect invalidates the channel, causing in-flight
        // PassThruReadMsgs / PassThruWriteMsgs to fail on subsequent polls.
        let status = Status::from(unsafe { (self.pass_thru_disconnect)(self.channel_id) });
        tracing::trace!(ret = %status, "PassThruDisconnect");
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
        if !self.connected {
            return Ok(());
        }

        while let Some(frame) = frames.pop_front() {
            let arb_id: u32 = frame.id.into();
            tracing::debug!(
                id = format_args!("{arb_id:08X}"),
                payload = %hex::encode(&frame.data),
                "J2534 TX"
            );

            let mut msg = PassThruMsg::new(Protocol::Can.into(), frame.id, &frame.data);
            let mut count: u32 = 1;

            let status = Status::from(unsafe {
                (self.write)(self.channel_id, &mut msg, &mut count, IO_TIMEOUT_MS)
            });
            tracing::trace!(ret = %status, count, "PassThruWriteMsgs");

            if status == Status::NoError && count == 1 {
                let mut loopback = frame.clone();
                loopback.loopback = true;
                self.loopback_queue.push_back(loopback);
            } else if matches!(
                status,
                Status::NoError | Status::Timeout | Status::BufferFull
            ) {
                // Anything short of a confirmed single-frame write is treated
                // as backpressure and retried on the next process iteration.
                frames.push_front(frame);
                break;
            } else {
                tracing::debug!(
                    ret = %status,
                    "J2534 TX error — channel disconnected, stopping adapter"
                );
                self.connected = false;
                frames.push_front(frame);
                return Ok(());
            }
        }
        Ok(())
    }

    fn recv(&mut self) -> crate::Result<Vec<Frame>> {
        if !self.connected {
            return Err(crate::Error::Disconnected);
        }

        let mut frames = Vec::new();
        loop {
            let mut msg = PassThruMsg::default();
            let mut count: u32 = 1;
            let status = Status::from(unsafe {
                (self.read)(self.channel_id, &mut msg, &mut count, IO_TIMEOUT_MS)
            });

            match status {
                Status::NoError if count > 0 => {
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

                        if let Ok(frame) = Frame::new(0, id, &data) {
                            frames.push(frame);
                        }
                    }
                }
                Status::NoError | Status::Timeout | Status::BufferEmpty => break,
                _ => {
                    tracing::debug!(
                        ret = %status,
                        "J2534 RX error — channel disconnected, stopping adapter"
                    );
                    self.connected = false;
                    return Err(crate::Error::Disconnected);
                }
            }
        }

        for mut frame in self.loopback_queue.drain(..) {
            frame.loopback = true;
            frames.push(frame);
        }

        Ok(frames)
    }
}
