//! Raw-USB driver for the PEAK PCAN-USB FD family of CAN adapters.
//!
//! This talks the "uCAN" protocol directly over USB bulk endpoints via the
//! [`crate::usb::UsbBackend`] abstraction: [`rusb`](https://crates.io/crates/rusb) on
//! native platforms, or WebUSB in the browser. It does not rely on the in-kernel
//! `peak_usb` driver or SocketCAN.
//!
//! On native platforms the in-kernel driver is detached automatically. **WebUSB cannot
//! detach kernel drivers**, so on Linux the device must be unbound from `peak_usb` (or it
//! blocklisted) before a browser can claim it. Non-root native USB access needs a udev
//! rule (see the crate README).
//!
//! Both classic CAN and CAN-FD are supported, including full control over the nominal and
//! data-phase bit timing via [`crate::can::bitrate::BitrateBuilder`].

mod constants;
pub mod error;
mod protocol;

pub use error::Error;

use std::collections::VecDeque;
use std::time::Duration;

use crate::can::bitrate::{AdapterTimingConst, BitTimingConst, BitrateConfig};
use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
use crate::peak::constants::*;
use crate::usb::{ControlType, Recipient, UsbBackend};
use crate::Result;
use tracing::{info, warn};

#[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
use crate::usb::RusbBackend;
#[cfg(all(target_arch = "wasm32", feature = "webusb"))]
use crate::usb::WebUsbBackend;

const CMD_TIMEOUT: Duration = Duration::from_millis(1000);
const TX_TIMEOUT: Duration = Duration::from_millis(1000);
const RX_TIMEOUT: Duration = Duration::from_millis(5);
#[cfg(not(target_arch = "wasm32"))]
const FLUSH_TIMEOUT: Duration = Duration::from_millis(10);

/// Maximum number of command records per USB transfer, leaving room for the
/// trailing end-of-collection marker.
const MAX_CMDS_PER_TRANSFER: usize = CMD_BUFFER_SIZE / COMMAND_SIZE - 1;

/// Number of frames the device can reliably hold in flight (transmitted but not
/// yet read back) before its internal FIFO overruns. The [`AsyncCanAdapter`]
/// uses this (via [`CanAdapter::buffer_size`]) to throttle transmission. Chosen
/// conservatively below the observed hardware capacity.
const PEAK_BUFFER_SIZE: usize = 128;

/// The PCAN-USB FD family uses a fixed 80 MHz controller clock.
const PEAK_NOMINAL_TIMING: BitTimingConst = BitTimingConst {
    clock_hz: CLOCK_HZ,
    tseg1_min: 1,
    tseg1_max: 1 << 8,
    tseg2_min: 1,
    tseg2_max: 1 << 7,
    sjw_max: 1 << 7,
    brp_min: 1,
    brp_max: 1 << 10,
    brp_inc: 1,
};

const PEAK_DATA_TIMING: BitTimingConst = BitTimingConst {
    clock_hz: CLOCK_HZ,
    tseg1_min: 1,
    tseg1_max: 1 << 5,
    tseg2_min: 1,
    tseg2_max: 1 << 4,
    sjw_max: 1 << 4,
    brp_min: 1,
    brp_max: 1 << 10,
    brp_inc: 1,
};

/// Timing constants for the PEAK PCAN-USB FD, usable with
/// [`crate::can::bitrate::BitrateBuilder::with_timing_const`].
pub const PEAK_TIMING_CONST: AdapterTimingConst = AdapterTimingConst {
    nominal: PEAK_NOMINAL_TIMING,
    data: Some(PEAK_DATA_TIMING),
};

/// The default [`UsbBackend`] for the current target: [`RusbBackend`] on native, or
/// [`WebUsbBackend`] when targeting the browser (`wasm32`).
#[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
pub type DefaultBackend = crate::usb::RusbBackend;
#[cfg(all(target_arch = "wasm32", feature = "webusb"))]
pub type DefaultBackend = crate::usb::WebUsbBackend;

/// PEAK PCAN-USB FD adapter, generic over the [`UsbBackend`] used for USB transfers.
pub struct Peak<B: UsbBackend = DefaultBackend> {
    backend: B,
    /// CAN channel index (0 for single-channel devices like the PCAN-USB FD).
    channel: u8,
    /// USB endpoints in use (resolved from firmware info or defaults).
    ep_cmd_out: u8,
    ep_msg_out: u8,
    ep_msg_in: u8,
    /// Whether CAN-FD frames should request a bitrate switch (data bitrate set).
    use_brs: bool,
    /// Whether the firmware supports toggling ISO / non-ISO CAN-FD framing.
    iso_fd_supported: bool,
}

// SAFETY: only applies when the backend is `Send` (native `RusbBackend`).
unsafe impl<B: UsbBackend + Send> Send for Peak<B> {}

impl<B: UsbBackend> Peak<B> {
    /// Read firmware info, resolve endpoints, take control of the device, configure clock,
    /// bit timing and filters, go bus-on, and flush any stale frames.
    async fn setup(backend: B, bitrate_cfg: &BitrateConfig) -> Result<Peak<B>> {
        let (fw_info, fw_len) = read_fw_info(&backend).await?;
        let fw_type = u16::from_le_bytes([
            fw_info[FW_INFO_TYPE_OFFSET],
            fw_info[FW_INFO_TYPE_OFFSET + 1],
        ]);
        let fw_major = fw_info[FW_INFO_FW_VERSION_OFFSET];
        let iso_fd_supported = fw_major >= 2;

        let (ep_cmd_out, ep_msg_out, ep_msg_in) =
            if fw_type >= FW_INFO_TYPE_EXT && fw_len >= FW_INFO_LEN {
                (
                    fw_info[FW_INFO_CMD_OUT_EP_OFFSET] & 0x7f,
                    fw_info[FW_INFO_DATA_OUT_EP_OFFSET] & 0x7f,
                    fw_info[FW_INFO_DATA_IN_EP_OFFSET] | 0x80,
                )
            } else {
                (DEFAULT_EP_CMD_OUT, DEFAULT_EP_MSG_OUT, DEFAULT_EP_MSG_IN)
            };

        let peak = Peak {
            backend,
            channel: 0,
            ep_cmd_out,
            ep_msg_out,
            ep_msg_in,
            use_brs: bitrate_cfg.data.is_some(),
            iso_fd_supported,
        };

        // Tell the device a host driver is now in control.
        peak.set_driver_loaded(true).await?;

        // Apply clock, bit timing, filters and go bus-on.
        peak.configure(bitrate_cfg).await?;

        // Drop anything buffered from before we took over.
        peak.flush_rx().await?;

        info!(
            "Connected to PEAK PCAN-USB FD (fw v{}.{}.{}, channel {})",
            fw_info[FW_INFO_FW_VERSION_OFFSET],
            fw_info[FW_INFO_FW_VERSION_OFFSET + 1],
            fw_info[FW_INFO_FW_VERSION_OFFSET + 2],
            peak.channel,
        );

        Ok(peak)
    }

    /// Send a list of command records, terminated with an end-of-collection
    /// marker, split across USB transfers no larger than the command buffer.
    async fn send_commands(&self, commands: &[protocol::Command]) -> Result<()> {
        for chunk in commands.chunks(MAX_CMDS_PER_TRANSFER) {
            let mut buf = Vec::with_capacity((chunk.len() + 1) * COMMAND_SIZE);
            for cmd in chunk {
                buf.extend_from_slice(cmd);
            }
            buf.extend_from_slice(&protocol::end_of_collection());

            // A command list must be delivered as one complete transfer; a short
            // write would leave the device with a torn command record. Fail loudly
            // rather than silently misconfigure it.
            let written = self
                .backend
                .write_bulk(self.ep_cmd_out, &buf, CMD_TIMEOUT)
                .await?;
            if written != buf.len() {
                return Err(Error::IncompleteWrite {
                    written,
                    expected: buf.len(),
                }
                .into());
            }
        }
        Ok(())
    }

    /// Tell the device whether a host driver is loaded (vendor control request).
    async fn set_driver_loaded(&self, loaded: bool) -> Result<()> {
        let mut buf = [0u8; FCT_DRV_LOADED_LEN];
        buf[1] = loaded as u8;
        self.backend
            .write_control(
                ControlType::Vendor,
                Recipient::Other,
                CTRL_REQ_FCT,
                FCT_DRV_LOADED,
                0,
                &buf,
                CMD_TIMEOUT,
            )
            .await
    }

    /// Configure clock, bit timing and filters, then bring the bus up.
    async fn configure(&self, cfg: &BitrateConfig) -> Result<()> {
        let ch = self.channel;

        // Select the 80 MHz clock domain.
        self.send_commands(&[protocol::cmd_set_clock(ch, CLOCK_80MHZ)])
            .await?;

        // Bit timing can only be set while in reset (bus-off) mode.
        self.send_commands(&[protocol::cmd_reset_mode(ch)]).await?;
        self.send_commands(&[protocol::cmd_timing_slow(ch, &cfg.nominal)])
            .await?;
        if let Some(data) = &cfg.data {
            self.send_commands(&[protocol::cmd_timing_fast(ch, data)])
                .await?;
        }

        // Accept every standard ID. Extended IDs are not affected by this filter.
        self.send_commands(&protocol::cmd_filter_accept_all(ch))
            .await?;

        // Bring the bus up: clear error counters, select ISO CAN-FD framing if
        // the firmware supports it, then enter normal mode.
        let mut bus_on = vec![protocol::cmd_reset_error_counters(ch)];
        if self.iso_fd_supported {
            bus_on.push(protocol::cmd_set_fd_iso(ch, true));
        }
        bus_on.push(protocol::cmd_normal_mode(ch));
        self.send_commands(&bus_on).await?;

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn set_bus_off(&self) -> Result<()> {
        self.send_commands(&[protocol::cmd_reset_mode(self.channel)])
            .await
    }

    /// Drain any frames the device has already buffered.
    ///
    /// The PEAK device NAKs (rather than sending a zero-length packet) when no frames are
    /// buffered; on native that surfaces as an empty read via the libusb timeout. WebUSB
    /// has no transfer timeout, so the drain would block forever — skip it there (a few
    /// stale frames at startup are harmless).
    async fn flush_rx(&self) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        loop {
            let data = self
                .backend
                .read_bulk(self.ep_msg_in, RX_BUFFER_SIZE, FLUSH_TIMEOUT)
                .await?;
            if data.is_empty() {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
impl Peak<RusbBackend> {
    /// Connect to the first available PCAN-USB FD family adapter and wrap it in an
    /// [`AsyncCanAdapter`]. The in-kernel `peak_usb` driver, if bound, is detached.
    pub fn new_async(bitrate_cfg: BitrateConfig) -> Result<AsyncCanAdapter> {
        let peak = Peak::new(bitrate_cfg)?;
        Ok(AsyncCanAdapter::new(peak))
    }

    /// Connect to the first available PCAN-USB FD family adapter.
    pub fn new(bitrate_cfg: BitrateConfig) -> Result<Peak<RusbBackend>> {
        let backend = RusbBackend::open_first(&[USB_VID], SUPPORTED_PIDS)?;
        pollster::block_on(Peak::setup(backend, &bitrate_cfg))
    }
}

#[cfg(all(target_arch = "wasm32", feature = "webusb"))]
impl Peak<WebUsbBackend> {
    /// Prompt the user to select a PCAN-USB FD adapter over WebUSB and wrap it in an
    /// [`AsyncCanAdapter`]. Must be called from a user gesture. On Linux the device must
    /// be unbound from the `peak_usb` kernel driver first.
    pub async fn connect_async(bitrate_cfg: BitrateConfig) -> Result<AsyncCanAdapter> {
        let peak = Peak::connect(bitrate_cfg).await?;
        Ok(AsyncCanAdapter::new(peak))
    }

    /// Prompt the user to select a PCAN-USB FD adapter over WebUSB and connect to it.
    pub async fn connect(bitrate_cfg: BitrateConfig) -> Result<Peak<WebUsbBackend>> {
        let backend = WebUsbBackend::request(&[USB_VID], SUPPORTED_PIDS).await?;
        Peak::setup(backend, &bitrate_cfg).await
    }
}

impl<B: UsbBackend> CanAdapter for Peak<B> {
    async fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
        while !frames.is_empty() {
            // Pack as many frames as fit into a single USB transfer, remembering
            // the buffer offset at which each frame's record ends.
            let mut buf = Vec::with_capacity(TX_BUFFER_SIZE);
            let mut record_ends: Vec<usize> = Vec::new();

            for frame in frames.iter() {
                let record = protocol::encode_tx_frame(frame, self.use_brs)?;
                // Always keep room for the 4-byte zero terminator record.
                if !record_ends.is_empty() && buf.len() + record.len() + 4 > TX_BUFFER_SIZE {
                    break;
                }
                buf.extend_from_slice(&record);
                record_ends.push(buf.len());
            }
            buf.extend_from_slice(&[0u8; 4]); // zero-size record: end of list

            // `write_bulk` returns the number of bytes accepted (0 on a timeout when the
            // device buffer is full). Only frames whose record was fully transmitted are
            // sent; a short write leaves the rest queued to retry. The device parses
            // records up to the transfer length and drops any torn trailing record, so a
            // re-sent frame arrives intact.
            let written = self
                .backend
                .write_bulk(self.ep_msg_out, &buf, TX_TIMEOUT)
                .await?;
            let sent = record_ends
                .iter()
                .take_while(|&&end| end <= written)
                .count();
            for _ in 0..sent {
                frames.pop_front();
            }
            if sent < record_ends.len() {
                // Short write or no space: leave the remaining frames queued.
                break;
            }
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<Frame>> {
        let data = self
            .backend
            .read_bulk(self.ep_msg_in, RX_BUFFER_SIZE, RX_TIMEOUT)
            .await?;

        let (frames, overruns) = protocol::parse_rx_buffer(&data);
        if overruns > 0 {
            warn!(
                "PEAK reported {} RX overrun message(s); frames may be lost",
                overruns
            );
        }
        Ok(frames)
    }

    /// The device has a finite TX/RX FIFO, so cap the number of frames in flight.
    fn buffer_size(&self) -> Option<usize> {
        Some(PEAK_BUFFER_SIZE)
    }

    fn timing_const() -> AdapterTimingConst
    where
        Self: Sized,
    {
        PEAK_TIMING_CONST
    }
}

impl<B: UsbBackend> Drop for Peak<B> {
    fn drop(&mut self) {
        // Best-effort: take the bus down and tell the device the driver is gone. Only on
        // native, where the async USB calls can be driven to completion synchronously; in
        // the browser the device resets when the page/connection goes away.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = pollster::block_on(self.set_bus_off());
            let _ = pollster::block_on(self.set_driver_loaded(false));
        }
    }
}

/// Read the firmware-info structure via a vendor control request. Returns the buffer and
/// the number of bytes actually read.
async fn read_fw_info<B: UsbBackend>(backend: &B) -> Result<([u8; FW_INFO_LEN], usize)> {
    let data = backend
        .read_control(
            ControlType::Vendor,
            Recipient::Other,
            CTRL_REQ_INFO,
            INFO_FW,
            0,
            FW_INFO_LEN,
            CMD_TIMEOUT,
        )
        .await?;

    let mut buf = [0u8; FW_INFO_LEN];
    let n = data.len().min(FW_INFO_LEN);
    buf[..n].copy_from_slice(&data[..n]);
    Ok((buf, n))
}
