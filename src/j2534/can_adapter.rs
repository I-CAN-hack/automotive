//! SAE J2534 PassThru CAN adapter.
//!
//! Implements [`CanAdapter`] directly on top of a J2534 CAN channel and relies
//! on [`crate::can::AsyncCanAdapter`] for background polling, retry logic, and
//! async send/receive orchestration.

use super::common::{self, parse_can_id, J2534Channel, J2534Device, PassThruMsg};
use super::constants::{Protocol, Status};
use super::error::Error as J2534Error;
use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
use crate::Result;

use std::collections::VecDeque;

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
    channel: J2534Channel,
    connected: bool,
    device: Option<J2534Device>,
}

impl J2534CanAdapter {
    /// Creates a new [`AsyncCanAdapter`] from a J2534CanAdapter
    pub fn new_async(dll_path: Option<&str>, bitrate: u32) -> Result<AsyncCanAdapter> {
        let socket = J2534CanAdapter::new(dll_path, bitrate)?;
        Ok(AsyncCanAdapter::new(socket))
    }

    /// Open a J2534 CAN channel.
    ///
    /// Opens a new device via `common::open_device`. To reuse an already-open
    /// device, use [`new_on_device`](Self::new_on_device).
    ///
    /// * `dll_path` — path to the PassThru DLL, or `None` to auto-discover
    ///   the first 64-bit driver from the Windows registry.
    /// * `bitrate` — CAN bitrate in bits/sec (e.g. `500_000`).
    pub fn new(dll_path: Option<&str>, bitrate: u32) -> Result<Self> {
        let device = common::open_device(dll_path)?;
        Self::new_on_device(device, bitrate)
    }

    /// Open a CAN channel on an already-open [`J2534Device`].
    ///
    /// This avoids closing and reopening the physical device when switching
    /// channels (e.g. reusing the same adapter for CAN after an ISO 15765
    /// channel was closed).
    ///
    pub fn new_on_device(device: J2534Device, bitrate: u32) -> Result<Self> {
        let channel = common::connect_channel(&device, Protocol::Can, bitrate)?;

        let status = channel.install_pass_all_can_filter();
        if status != Status::NoError {
            let _ = channel.disconnect();
            return Err(J2534Error::DllError(format!(
                "PassThruStartMsgFilter (PASS, pass-all) failed: {status}"
            ))
            .into());
        }

        Ok(Self {
            loopback_queue: VecDeque::new(),
            channel,
            connected: true,
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
        let status = self.channel.disconnect();
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
            return Err(crate::Error::Disconnected);
        }

        while let Some(frame) = frames.pop_front() {
            let arb_id: u32 = frame.id.into();
            tracing::debug!(
                id = format_args!("{arb_id:08X}"),
                payload = %hex::encode(&frame.data),
                "J2534 TX"
            );

            let mut msg = PassThruMsg::new(Protocol::Can.into(), frame.id, &frame.data);
            let (status, count) = self.channel.write_message(&mut msg, IO_TIMEOUT_MS);
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
            let (status, count) = self.channel.read_message(&mut msg, IO_TIMEOUT_MS);

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

        // Add fake loopback frames to the receive queue
        frames.extend(self.loopback_queue.drain(..));

        Ok(frames)
    }
}
