//! Raw-USB driver for the PEAK PCAN-USB FD family of CAN adapters.
//!
//! This talks the "uCAN" protocol directly over USB bulk endpoints using
//! [rusb](https://crates.io/crates/rusb), without relying on the in-kernel
//! `peak_usb` driver or SocketCAN. Opening fails if a kernel driver owns the
//! USB interface unless [`PeakConfig::detach_kernel_driver`] is enabled.
//!
//! On Linux, accessing the USB device as a non-root user requires a udev rule
//! granting access (see the crate README).
//!
//! Both classic CAN and CAN-FD are supported, including full control over the
//! nominal and data-phase bit timing via [`crate::can::bitrate::BitrateBuilder`].

mod config;
mod constants;
mod error;
mod protocol;

pub use config::{PeakConfig, PeakConfigBuilder};
pub use error::Error;

use std::collections::VecDeque;
use std::time::Duration;

use crate::adapters::peak::constants::*;
use crate::adapters::validate_tx_timeout;
use crate::can::bitrate::{AdapterTimingConst, BitTimingConst, BitrateConfig};
use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
use crate::Result;
use tracing::{info, warn};

const CMD_TIMEOUT: Duration = Duration::from_millis(1000);
const TX_TIMEOUT: Duration = Duration::from_millis(1000);
// Keep an idle receive from delaying the worker's next send for several ms.
const RX_TIMEOUT: Duration = Duration::from_millis(1);
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

const PEAK_TIMING_CONST: AdapterTimingConst = AdapterTimingConst {
    nominal: PEAK_NOMINAL_TIMING,
    data: Some(PEAK_DATA_TIMING),
};

/// Blocking implementation of the PEAK PCAN-USB FD adapter.
pub struct PeakAdapter {
    tx_timeout: Option<Duration>,
    /// Hardware TX may contain frames until the next successful queue clear.
    tx_queue_dirty: bool,
    buffered_rx: VecDeque<Frame>,
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    /// CAN channel index (0 for single-channel devices like the PCAN-USB FD).
    channel: u8,
    /// USB endpoints in use (resolved from firmware info or defaults).
    ep_cmd_out: u8,
    ep_msg_out: u8,
    ep_msg_in: u8,
    /// Whether a configured data bitrate permits bitrate switching.
    use_brs: bool,
    /// Whether the firmware supports toggling ISO / non-ISO CAN-FD framing.
    iso_fd_supported: bool,
}

// rusb's GlobalContext handle is safe to move between threads.
unsafe impl Send for PeakAdapter {}

impl PeakAdapter {
    /// Connect to the first available PCAN-USB FD family adapter and wrap it in
    /// an [`AsyncCanAdapter`].
    pub fn new_async(config: PeakConfig) -> Result<AsyncCanAdapter> {
        let peak = PeakAdapter::new(config)?;
        Ok(AsyncCanAdapter::new(peak))
    }

    /// Connect to the first available PCAN-USB FD family adapter.
    ///
    /// Returns a USB busy error if a kernel driver owns the interface.
    /// Set [`PeakConfig::detach_kernel_driver`] to true to allow detaching
    /// that driver while the adapter is open.
    pub fn new(config: PeakConfig) -> Result<PeakAdapter> {
        let bitrate_cfg = config.bitrate;
        validate_tx_timeout(config.tx_timeout)?;
        for device in rusb::devices()?.iter() {
            let desc = match device.device_descriptor() {
                Ok(desc) => desc,
                Err(_) => continue,
            };

            if desc.vendor_id() != USB_VID || !SUPPORTED_PIDS.contains(&desc.product_id()) {
                continue;
            }

            let handle = device.open()?;

            // Detaching a kernel driver takes the device away from SocketCAN.
            // Only enable that behavior with explicit opt-in.
            if config.detach_kernel_driver {
                handle.set_auto_detach_kernel_driver(true).ok();
            }
            handle.claim_interface(0)?;

            // Read firmware info to learn the firmware version and (on newer
            // firmware) which endpoints to use.
            let (fw_info, fw_len) = read_fw_info(&handle)?;
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

            let mut peak = PeakAdapter {
                tx_timeout: config.tx_timeout,
                // A previous process may have left transmissions in hardware.
                tx_queue_dirty: true,
                buffered_rx: VecDeque::new(),
                handle,
                channel: 0,
                ep_cmd_out,
                ep_msg_out,
                ep_msg_in,
                use_brs: bitrate_cfg.data.is_some(),
                iso_fd_supported,
            };

            // Tell the device a host driver is now in control.
            peak.set_driver_loaded(true)?;

            // Configure while reset, then clear old TX before going bus-on.
            peak.configure(&bitrate_cfg)?;

            // Drop anything buffered from before we took over and go bus-on.
            peak.cancel_pending_tx()?;

            info!(
                "Connected to PEAK PCAN-USB FD (fw v{}.{}.{}, channel {})",
                fw_info[FW_INFO_FW_VERSION_OFFSET],
                fw_info[FW_INFO_FW_VERSION_OFFSET + 1],
                fw_info[FW_INFO_FW_VERSION_OFFSET + 2],
                peak.channel,
            );

            return Ok(peak);
        }

        Err(crate::Error::NotFound)
    }

    /// Send a list of command records, terminated with an end-of-collection
    /// marker, split across USB transfers no larger than the command buffer.
    fn send_commands(&self, commands: &[protocol::Command]) -> Result<()> {
        for chunk in commands.chunks(MAX_CMDS_PER_TRANSFER) {
            let mut buf = Vec::with_capacity((chunk.len() + 1) * COMMAND_SIZE);
            for cmd in chunk {
                buf.extend_from_slice(cmd);
            }
            buf.extend_from_slice(&protocol::end_of_collection());

            // A command list must be delivered as one complete transfer; a short
            // write would leave the device with a torn command record. Fail loudly
            // rather than silently misconfigure it.
            let written = self.handle.write_bulk(self.ep_cmd_out, &buf, CMD_TIMEOUT)?;
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
    fn set_driver_loaded(&self, loaded: bool) -> Result<()> {
        let mut buf = [0u8; FCT_DRV_LOADED_LEN];
        buf[1] = loaded as u8;

        let request_type = rusb::request_type(
            rusb::Direction::Out,
            rusb::RequestType::Vendor,
            rusb::Recipient::Other,
        );
        self.handle.write_control(
            request_type,
            CTRL_REQ_FCT,
            FCT_DRV_LOADED,
            0,
            &buf,
            CMD_TIMEOUT,
        )?;
        Ok(())
    }

    /// Configure clock, bit timing and filters while keeping the bus off.
    fn configure(&self, cfg: &BitrateConfig) -> Result<()> {
        let ch = self.channel;

        // Select the 80 MHz clock domain.
        self.send_commands(&[protocol::cmd_set_clock(ch, CLOCK_80MHZ)])?;

        // Bit timing can only be set while in reset (bus-off) mode.
        self.send_commands(&[protocol::cmd_reset_mode(ch)])?;
        // Tell the USB CANFD interfaces to transfer the received frames as
        // soon as they arrive rather than waiting a while to agglomerate them
        // and thus minimize the USB traffic. Its default value is 0 (no fast
        // forward).
        self.send_commands(&[protocol::cmd_set_fast_forward(ch, true)])?;
        self.send_commands(&[protocol::cmd_timing_slow(ch, &cfg.nominal)])?;
        if let Some(data) = &cfg.data {
            self.send_commands(&[protocol::cmd_timing_fast(ch, data)])?;
        }

        // Accept every standard ID. Extended IDs are not affected by this filter.
        self.send_commands(&protocol::cmd_filter_accept_all(ch))?;

        Ok(())
    }

    fn set_bus_on(&self) -> Result<()> {
        let ch = self.channel;
        // Bring the bus up: clear error counters, select ISO CAN-FD framing if
        // the firmware supports it, then enter normal mode.
        let mut bus_on = vec![protocol::cmd_reset_error_counters(ch)];
        if self.iso_fd_supported {
            bus_on.push(protocol::cmd_set_fd_iso(ch, true));
        }
        bus_on.push(protocol::cmd_normal_mode(ch));
        self.send_commands(&bus_on)
    }

    fn set_bus_off(&self) -> Result<()> {
        self.send_commands(&[protocol::cmd_reset_mode(self.channel)])
    }
}

impl CanAdapter for PeakAdapter {
    fn default_brs(&self) -> bool {
        self.use_brs
    }

    fn supports_tx_timeout() -> bool {
        true
    }

    fn tx_timeout(&self) -> Option<Duration> {
        self.tx_timeout
    }

    fn cancel_pending_tx(&mut self) -> Result<()> {
        // new() already clears a fresh adapter. Wrapping it in AsyncCanAdapter
        // must not repeat that handshake unless sync sends occurred meanwhile.
        if !self.tx_queue_dirty {
            return Ok(());
        }
        // The kernel pcan_usb_fd_set_bus() and PEAK canfd_set_bus_off() use
        // RESET_MODE for USB. RB replies are synchronized in their PCIe paths
        // (peak_pciefd_main.c / pcanfd_pci.c), not their USB paths. Don't make
        // USB opening/recovery depend on an RX_BARRIER reply. Stop the core and
        // flush TX before draining old completions, then restore bus operation.
        self.send_commands(&[
            protocol::cmd_reset_mode(self.channel),
            protocol::cmd_abort_tx(self.channel),
        ])?;
        self.buffered_rx.retain(|frame| !frame.loopback);
        let deadline = std::time::Instant::now() + CMD_TIMEOUT;
        let mut buf = [0u8; RX_BUFFER_SIZE];
        while std::time::Instant::now() < deadline {
            match self
                .handle
                .read_bulk(self.ep_msg_in, &mut buf, FLUSH_TIMEOUT)
            {
                Ok(0) | Err(rusb::Error::Timeout) => {
                    self.set_bus_on()?;
                    self.tx_queue_dirty = false;
                    return Ok(());
                }
                Ok(n) => {
                    let data = &buf[..n];
                    let (frames, _) = protocol::parse_rx_buffer(data);
                    // Preserve bus traffic, but discard old TX completions.
                    self.buffered_rx
                        .extend(frames.into_iter().filter(|frame| !frame.loopback));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(crate::Error::Timeout)
    }

    fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
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

            // A short write or USB timeout may still submit part of the buffer.
            self.tx_queue_dirty = true;
            match self.handle.write_bulk(self.ep_msg_out, &buf, TX_TIMEOUT) {
                Ok(written) => {
                    // Only frames whose record was fully transmitted are sent;
                    // a short write leaves the rest queued to retry. The device
                    // parses records up to the transfer length and drops any torn
                    // trailing record, so a re-sent frame arrives intact.
                    let sent = record_ends
                        .iter()
                        .take_while(|&&end| end <= written)
                        .count();
                    for _ in 0..sent {
                        frames.pop_front();
                    }
                    if sent < record_ends.len() {
                        // Short write: leave the remaining frames queued.
                        break;
                    }
                }
                // No space right now: leave frames queued and retry next call.
                Err(rusb::Error::Timeout) => break,
                Err(rusb::Error::NoDevice) => return Err(crate::Error::Disconnected),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<Frame>> {
        let mut buf = [0u8; RX_BUFFER_SIZE];

        let n = match self.handle.read_bulk(self.ep_msg_in, &mut buf, RX_TIMEOUT) {
            Ok(n) => n,
            Err(rusb::Error::Timeout) => return Ok(self.buffered_rx.drain(..).collect()),
            Err(rusb::Error::NoDevice) => return Err(crate::Error::Disconnected),
            Err(e) => return Err(e.into()),
        };

        let (frames, overruns) = protocol::parse_rx_buffer(&buf[..n]);
        if overruns > 0 {
            warn!(
                "PEAK reported {} RX overrun message(s); frames may be lost",
                overruns
            );
        }
        self.buffered_rx.extend(frames);
        Ok(self.buffered_rx.drain(..).collect())
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

impl Drop for PeakAdapter {
    fn drop(&mut self) {
        // Best-effort: take the bus down and tell the device the driver is gone.
        let _ = self.set_bus_off();
        // This USB option persists across host drivers; restore its default.
        let _ = self.send_commands(&[protocol::cmd_set_fast_forward(self.channel, false)]);
        let _ = self.set_driver_loaded(false);
    }
}

/// Read the firmware-info structure via a vendor control request.
///
/// Returns the buffer and the number of bytes actually read.
fn read_fw_info(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
) -> Result<([u8; FW_INFO_LEN], usize)> {
    let mut buf = [0u8; FW_INFO_LEN];
    let request_type = rusb::request_type(
        rusb::Direction::In,
        rusb::RequestType::Vendor,
        rusb::Recipient::Other,
    );
    let n = handle.read_control(
        request_type,
        CTRL_REQ_INFO,
        INFO_FW,
        0,
        &mut buf,
        CMD_TIMEOUT,
    )?;
    Ok((buf, n))
}
