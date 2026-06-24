//! Panda CAN adapter support

mod constants;
mod error;
mod usb_protocol;

pub use error::Error;
use std::collections::VecDeque;
use std::time::Duration;

use crate::can::bitrate::{AdapterTimingConst, BitTimingConst, BitrateConfig};
use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
use crate::panda::constants::{Endpoint, HwType, SafetyModel};
use crate::usb::UsbBackend;
use crate::Result;
use tracing::{info, warn};

#[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
use crate::usb::RusbBackend;
#[cfg(all(target_arch = "wasm32", feature = "webusb"))]
use crate::usb::WebUsbBackend;

const USB_VIDS: &[u16] = &[0xbbaa, 0x3801];
const USB_PIDS: &[u16] = &[0xddee, 0xddcc];
// CAN packet versions known to use the wire format implemented in `usb_protocol`.
// Older firmware hardcoded an incrementing integer (4). Newer firmware derives the
// version from a hash of the packet definition, so it reports a different value (157)
// even though the actual packet layout is unchanged.
const SUPPORTED_CAN_PACKET_VERSIONS: &[u8] = &[4, 157];
const MAX_BULK_SIZE: usize = 16384;
const PANDA_BUS_CNT: usize = 3;
const PANDA_NOMINAL_SAMPLE_POINT: f64 = 0.8;
const SUPPORTED_CAN_BITRATES_BPS: &[u32] = &[
    10_000, 20_000, 50_000, 100_000, 125_000, 250_000, 500_000, 1_000_000,
];
const SUPPORTED_CAN_DATA_BITRATES_BPS: &[u32] = &[
    10_000, 20_000, 50_000, 100_000, 125_000, 250_000, 500_000, 1_000_000, 2_000_000, 5_000_000,
];
const PANDA_BIT_TIMING_CONST: BitTimingConst = BitTimingConst {
    clock_hz: 80_000_000,
    tseg1_min: 1,
    tseg1_max: 1 << 8,
    tseg2_min: 1,
    tseg2_max: 1 << 7,
    sjw_max: 1 << 7,
    brp_min: 1,
    brp_max: 1 << 10,
    brp_inc: 1,
};

/// Timing constants for the panda CAN controller, usable with
/// [`crate::can::bitrate::BitrateBuilder::with_timing_const`].
pub const PANDA_TIMING_CONST: AdapterTimingConst = AdapterTimingConst {
    nominal: PANDA_BIT_TIMING_CONST,
    data: Some(PANDA_BIT_TIMING_CONST),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PandaBitrateConfig {
    nominal_kbps: u16,
    data_kbps: Option<u16>,
}

/// The default [`UsbBackend`] for the current target: [`RusbBackend`] on native
/// platforms, [`WebUsbBackend`] when targeting the browser (`wasm32`).
#[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
pub type DefaultBackend = crate::usb::RusbBackend;
#[cfg(all(target_arch = "wasm32", feature = "webusb"))]
pub type DefaultBackend = crate::usb::WebUsbBackend;

/// Panda CAN adapter, generic over the [`UsbBackend`] used for USB transfers.
///
/// On native platforms `B` defaults to [`RusbBackend`] (libusb) and the adapter implements
/// the blocking [`CanAdapter`] trait. When targeting the browser, `B` is
/// [`WebUsbBackend`] and frames are sent/received with the async [`Panda::send_frames`] /
/// [`Panda::recv_frames`] methods.
pub struct Panda<B: UsbBackend = DefaultBackend> {
    backend: B,
    timeout: Duration,
    dat: Vec<u8>,
}

#[allow(dead_code)]
struct Versions {
    health_version: u8,
    can_version: u8,
    can_health_version: u8,
}

impl<B: UsbBackend> Panda<B> {
    /// Runs the post-connection setup sequence: validates firmware, configures safety
    /// mode and bus bitrates, and flushes the receive buffer. Returns the hardware type.
    async fn configure(&self, cfg: PandaBitrateConfig) -> Result<HwType> {
        // Check panda firmware version
        let versions = self.get_packets_versions().await?;
        if !SUPPORTED_CAN_PACKET_VERSIONS.contains(&versions.can_version) {
            return Err(Error::WrongFirmwareVersion.into());
        }

        let hw_type = self.get_hw_type().await?;
        warn_if_fd_unsupported(hw_type, cfg.data_kbps.is_some());

        self.set_safety_model(SafetyModel::AllOutput).await?;
        self.set_power_save(false).await?;
        self.set_heartbeat_disabled().await?;
        self.can_reset_communications().await?;

        for i in 0..PANDA_BUS_CNT {
            self.set_can_speed_kbps(i, cfg.nominal_kbps).await?;
            if let Some(data_kbps) = cfg.data_kbps {
                self.set_can_data_speed_kbps(i, data_kbps).await?;
            }
            self.set_canfd_auto(i, false).await?;
        }

        // can_reset_communications() doesn't work properly, flush manually
        self.flush_rx().await?;

        info!("Connected to Panda ({:?})", hw_type);
        Ok(hw_type)
    }

    /// Pack and send a queue of CAN frames over the bulk OUT endpoint.
    pub async fn send_frames(&self, frames: &mut VecDeque<Frame>) -> Result<()> {
        if frames.is_empty() {
            return Ok(());
        }

        let frames: Vec<Frame> = frames.drain(..).collect();
        let buf = usb_protocol::pack_can_buffer(&frames)?;

        for chunk in buf {
            self.backend
                .write_bulk(Endpoint::CanWrite as u8, &chunk, self.timeout)
                .await?;
        }
        Ok(())
    }

    /// Read and unpack the currently available CAN frames from the bulk IN endpoint. In
    /// case of a recoverable unpacking error, the buffer is cleared and an empty vector is
    /// returned.
    pub async fn recv_frames(&mut self) -> Result<Vec<Frame>> {
        let data = self
            .backend
            .read_bulk(Endpoint::CanRead as u8, MAX_BULK_SIZE, self.timeout)
            .await?;
        self.dat.extend_from_slice(&data);

        match usb_protocol::unpack_can_buffer(&mut self.dat) {
            Ok(frames) => Ok(frames),
            Err(e) => {
                warn!("Error unpacking: {:}", e);
                self.dat.clear();
                Ok(vec![])
            }
        }
    }

    async fn flush_rx(&self) -> Result<()> {
        loop {
            let data = self
                .backend
                .read_bulk(Endpoint::CanRead as u8, MAX_BULK_SIZE, self.timeout)
                .await?;
            if data.is_empty() {
                return Ok(());
            }
        }
    }

    /// Change the safety model of the panda. This can be useful to switch to Silent mode or open/close the relay in the comma.ai harness
    pub async fn set_safety_model(&self, safety_model: SafetyModel) -> Result<()> {
        let safety_param: u16 = 0;
        self.usb_write_control(Endpoint::SafetyModel, safety_model as u16, safety_param)
            .await
    }

    async fn set_heartbeat_disabled(&self) -> Result<()> {
        self.usb_write_control(Endpoint::HeartbeatDisabled, 0, 0)
            .await
    }

    async fn set_power_save(&self, power_save_enabled: bool) -> Result<()> {
        self.usb_write_control(Endpoint::PowerSave, power_save_enabled as u16, 0)
            .await
    }

    async fn set_canfd_auto(&self, bus: usize, auto: bool) -> Result<()> {
        if bus >= PANDA_BUS_CNT {
            return Err(crate::Error::NotSupported);
        }
        self.usb_write_control(Endpoint::CanFDAuto, bus as u16, auto as u16)
            .await
    }

    async fn set_can_speed_kbps(&self, bus: usize, speed_kbps: u16) -> Result<()> {
        if bus >= PANDA_BUS_CNT {
            return Err(crate::Error::NotSupported);
        }
        self.usb_write_control(Endpoint::CanSpeed, bus as u16, speed_kbps * 10)
            .await
    }

    async fn set_can_data_speed_kbps(&self, bus: usize, speed_kbps: u16) -> Result<()> {
        if bus >= PANDA_BUS_CNT {
            return Err(crate::Error::NotSupported);
        }
        self.usb_write_control(Endpoint::CanDataSpeed, bus as u16, speed_kbps * 10)
            .await
    }

    /// Get the hardware type of the panda. Usefull to detect if it supports CAN-FD.
    pub async fn get_hw_type(&self) -> Result<HwType> {
        let hw_type = self.usb_read_control(Endpoint::HwType, 1).await?;
        HwType::from_repr(hw_type[0]).ok_or(Error::UnknownHwType.into())
    }

    async fn get_packets_versions(&self) -> Result<Versions> {
        let versions = self.usb_read_control(Endpoint::PacketsVersions, 3).await?;
        Ok(Versions {
            health_version: versions[0],
            can_version: versions[1],
            can_health_version: versions[2],
        })
    }

    async fn can_reset_communications(&self) -> Result<()> {
        self.usb_write_control(Endpoint::CanResetCommunications, 0, 0)
            .await
    }

    async fn usb_read_control(&self, endpoint: Endpoint, n: usize) -> Result<Vec<u8>> {
        self.backend
            .read_control(endpoint as u8, 0, 0, n, self.timeout)
            .await
    }

    async fn usb_write_control(&self, endpoint: Endpoint, value: u16, index: u16) -> Result<()> {
        self.backend
            .write_control(endpoint as u8, value, index, &[], self.timeout)
            .await
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
impl Panda<RusbBackend> {
    /// Convenience function to create a new panda adapter and wrap in an [`AsyncCanAdapter`]
    pub fn new_async(bitrate_cfg: BitrateConfig) -> Result<AsyncCanAdapter> {
        let panda = Panda::new(bitrate_cfg)?;
        Ok(AsyncCanAdapter::new(panda))
    }

    /// Connect to the first available panda. This function will set the safety mode to ALL_OUTPUT and clear all buffers.
    pub fn new(bitrate_cfg: BitrateConfig) -> Result<Panda<RusbBackend>> {
        let resolved_bitrate_cfg = resolve_bitrate_config(&bitrate_cfg)?;
        warn_if_non_default_sample_points(&bitrate_cfg);

        let backend = RusbBackend::open_first(USB_VIDS, USB_PIDS)?;
        let panda = Panda {
            backend,
            timeout: Duration::from_millis(100),
            dat: vec![],
        };

        pollster::block_on(panda.configure(resolved_bitrate_cfg))?;
        Ok(panda)
    }
}

#[cfg(all(target_arch = "wasm32", feature = "webusb"))]
impl Panda<WebUsbBackend> {
    /// Prompt the user to select a panda over WebUSB, connect to it, and wrap it in an
    /// [`AsyncCanAdapter`]. Must be called from a user gesture (e.g. a button click).
    pub async fn connect_async(bitrate_cfg: BitrateConfig) -> Result<AsyncCanAdapter> {
        let panda = Panda::connect(bitrate_cfg).await?;
        Ok(AsyncCanAdapter::new(panda))
    }

    /// Prompt the user to select a panda over WebUSB and connect to it. Must be called
    /// from a user gesture (e.g. a button click). Sets the safety mode to ALL_OUTPUT and
    /// clears all buffers.
    pub async fn connect(bitrate_cfg: BitrateConfig) -> Result<Panda<WebUsbBackend>> {
        let resolved_bitrate_cfg = resolve_bitrate_config(&bitrate_cfg)?;
        warn_if_non_default_sample_points(&bitrate_cfg);

        let backend = WebUsbBackend::request(USB_VIDS, USB_PIDS).await?;
        let panda = Panda {
            backend,
            timeout: Duration::from_millis(100),
            dat: vec![],
        };

        panda.configure(resolved_bitrate_cfg).await?;
        Ok(panda)
    }
}

// SAFETY: only applies when the backend is `Send` (i.e. native `RusbBackend`). On wasm the
// `WebUsbBackend` holds JS values and is `!Send`, so this impl does not apply there.
unsafe impl<B: UsbBackend + Send> Send for Panda<B> {}

impl<B: UsbBackend> CanAdapter for Panda<B> {
    /// Sends a buffer of CAN messages to the panda.
    async fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
        self.send_frames(frames).await
    }

    /// Reads the current buffer of available CAN messages from the panda.
    async fn recv(&mut self) -> Result<Vec<Frame>> {
        self.recv_frames().await
    }

    fn timing_const() -> AdapterTimingConst
    where
        Self: Sized,
    {
        PANDA_TIMING_CONST
    }
}

fn resolve_bitrate_config(bitrate_cfg: &BitrateConfig) -> Result<PandaBitrateConfig> {
    Ok(PandaBitrateConfig {
        nominal_kbps: validate_supported_bitrate(
            bitrate_cfg.nominal.bitrate,
            SUPPORTED_CAN_BITRATES_BPS,
            "Panda arbitration bitrate",
        )?,
        data_kbps: bitrate_cfg
            .data
            .map(|data| {
                validate_supported_bitrate(
                    data.bitrate,
                    SUPPORTED_CAN_DATA_BITRATES_BPS,
                    "Panda data bitrate",
                )
            })
            .transpose()?,
    })
}

fn validate_supported_bitrate(bitrate_bps: u32, supported: &[u32], label: &str) -> Result<u16> {
    if !supported.contains(&bitrate_bps) {
        return Err(crate::Error::InvalidBitrate(format!(
            "{label} {bitrate_bps} bps is unsupported; expected one of {supported:?}"
        )));
    }

    Ok((bitrate_bps / 1000) as u16)
}

fn warn_if_non_default_sample_points(bitrate_cfg: &BitrateConfig) {
    if !sample_point_matches(bitrate_cfg.nominal.sample_point, PANDA_NOMINAL_SAMPLE_POINT) {
        warn!(
            bitrate = bitrate_cfg.nominal.bitrate,
            requested_sample_point = bitrate_cfg.nominal.sample_point,
            panda_sample_point = PANDA_NOMINAL_SAMPLE_POINT,
            "Panda ignores the requested nominal sample point"
        );
    }

    if let Some(data) = bitrate_cfg.data {
        let panda_sample_point = default_data_sample_point(data.bitrate);
        if !sample_point_matches(data.sample_point, panda_sample_point) {
            warn!(
                bitrate = data.bitrate,
                requested_sample_point = data.sample_point,
                panda_sample_point,
                "Panda ignores the requested CAN-FD data sample point"
            );
        }
    }
}

fn warn_if_fd_unsupported(hw_type: HwType, using_fd_bitrate: bool) {
    if using_fd_bitrate && !supports_can_fd(hw_type) {
        warn!(
            ?hw_type,
            "Configured CAN-FD bitrate on panda hardware that does not support CAN-FD"
        );
    }
}

fn supports_can_fd(hw_type: HwType) -> bool {
    !matches!(
        hw_type,
        HwType::Unknown
            | HwType::WhitePanda
            | HwType::GreyPanda
            | HwType::BlackPanda
            | HwType::Pedal
            | HwType::Uno
            | HwType::Dos
    )
}

fn default_data_sample_point(data_bitrate: u32) -> f64 {
    if data_bitrate == 5_000_000 {
        0.75
    } else {
        0.8
    }
}

fn sample_point_matches(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-9
}
