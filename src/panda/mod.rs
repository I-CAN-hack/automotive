//! Panda CAN adapter support

mod constants;
mod error;
mod usb_protocol;

pub use error::Error;
use std::collections::VecDeque;

use crate::can::bitrate::{AdapterTimingConst, BitTimingConst, BitrateConfig};
use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
use crate::panda::constants::{Endpoint, HwType, SafetyModel};
use crate::Result;
use tracing::{info, warn};

const USB_VIDS: &[u16] = &[0xbbaa, 0x3801];
const USB_PIDS: &[u16] = &[0xddee, 0xddcc];
const EXPECTED_CAN_PACKET_VERSION: u8 = 4;
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
const PANDA_TIMING_CONST: AdapterTimingConst = AdapterTimingConst {
    nominal: PANDA_BIT_TIMING_CONST,
    data: Some(PANDA_BIT_TIMING_CONST),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PandaBitrateConfig {
    nominal_kbps: u16,
    data_kbps: Option<u16>,
}

/// Blocking implementation of the panda CAN adapter
pub struct Panda {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    timeout: std::time::Duration,
    dat: Vec<u8>,
}

#[allow(dead_code)]
struct Versions {
    health_version: u8,
    can_version: u8,
    can_health_version: u8,
}

unsafe impl Send for Panda {}

impl Panda {
    /// Convenience function to create a new panda adapter and wrap in an [`AsyncCanAdapter`]
    pub fn new_async(bitrate_cfg: BitrateConfig) -> Result<AsyncCanAdapter> {
        let panda = Panda::new(bitrate_cfg)?;
        Ok(AsyncCanAdapter::new(panda))
    }

    /// Connect to the first available panda. This function will set the safety mode to ALL_OUTPUT and clear all buffers.
    pub fn new(bitrate_cfg: BitrateConfig) -> Result<Panda> {
        let resolved_bitrate_cfg = resolve_bitrate_config(&bitrate_cfg)?;
        warn_if_non_default_sample_points(&bitrate_cfg);

        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if !USB_VIDS.contains(&device_desc.vendor_id()) {
                continue;
            }
            if !USB_PIDS.contains(&device_desc.product_id()) {
                continue;
            }

            let panda = Panda {
                dat: vec![],
                handle: device.open()?,
                timeout: std::time::Duration::from_millis(100),
            };

            panda.handle.claim_interface(0)?;

            // Check panda firmware version
            let versions = panda.get_packets_versions()?;
            if versions.can_version != EXPECTED_CAN_PACKET_VERSION {
                return Err(Error::WrongFirmwareVersion.into());
            }

            let hw_type = panda.get_hw_type()?;
            warn_if_fd_unsupported(hw_type, resolved_bitrate_cfg.data_kbps.is_some());

            panda.set_safety_model(SafetyModel::AllOutput)?;
            panda.set_power_save(false)?;
            panda.set_heartbeat_disabled()?;
            panda.can_reset_communications()?;

            for i in 0..PANDA_BUS_CNT {
                panda.set_can_speed_kbps(i, resolved_bitrate_cfg.nominal_kbps)?;
                if let Some(data_kbps) = resolved_bitrate_cfg.data_kbps {
                    panda.set_can_data_speed_kbps(i, data_kbps)?;
                }
                panda.set_canfd_auto(i, false)?;
            }

            // can_reset_communications() doesn't work properly, flush manually
            panda.flush_rx()?;

            info!("Connected to Panda ({:?})", hw_type);

            return Ok(panda);
        }
        Err(crate::Error::NotFound)
    }

    fn flush_rx(&self) -> Result<()> {
        const N: usize = 16384;
        let mut buf: [u8; N] = [0; N];

        loop {
            let recv: usize =
                self.handle
                    .read_bulk(Endpoint::CanRead as u8, &mut buf, self.timeout)?;

            if recv == 0 {
                return Ok(());
            }
        }
    }

    /// Change the safety model of the panda. This can be useful to switch to Silent mode or open/close the relay in the comma.ai harness
    pub fn set_safety_model(&self, safety_model: SafetyModel) -> Result<()> {
        let safety_param: u16 = 0;
        self.usb_write_control(Endpoint::SafetyModel, safety_model as u16, safety_param)
    }

    fn set_heartbeat_disabled(&self) -> Result<()> {
        self.usb_write_control(Endpoint::HeartbeatDisabled, 0, 0)
    }

    fn set_power_save(&self, power_save_enabled: bool) -> Result<()> {
        self.usb_write_control(Endpoint::PowerSave, power_save_enabled as u16, 0)
    }

    fn set_canfd_auto(&self, bus: usize, auto: bool) -> Result<()> {
        if bus >= PANDA_BUS_CNT {
            return Err(crate::Error::NotSupported);
        }
        self.usb_write_control(Endpoint::CanFDAuto, bus as u16, auto as u16)
    }

    fn set_can_speed_kbps(&self, bus: usize, speed_kbps: u16) -> Result<()> {
        if bus >= PANDA_BUS_CNT {
            return Err(crate::Error::NotSupported);
        }
        self.usb_write_control(Endpoint::CanSpeed, bus as u16, speed_kbps * 10)
    }

    fn set_can_data_speed_kbps(&self, bus: usize, speed_kbps: u16) -> Result<()> {
        if bus >= PANDA_BUS_CNT {
            return Err(crate::Error::NotSupported);
        }
        self.usb_write_control(Endpoint::CanDataSpeed, bus as u16, speed_kbps * 10)
    }

    /// Get the hardware type of the panda. Usefull to detect if it supports CAN-FD.
    pub fn get_hw_type(&self) -> Result<HwType> {
        let hw_type = self.usb_read_control(Endpoint::HwType, 1)?;
        HwType::from_repr(hw_type[0]).ok_or(Error::UnknownHwType.into())
    }

    fn get_packets_versions(&self) -> Result<Versions> {
        let versions = self.usb_read_control(Endpoint::PacketsVersions, 3)?;
        Ok({
            Versions {
                health_version: versions[0],
                can_version: versions[1],
                can_health_version: versions[2],
            }
        })
    }

    fn can_reset_communications(&self) -> Result<()> {
        self.usb_write_control(Endpoint::CanResetCommunications, 0, 0)
    }

    fn usb_read_control(&self, endpoint: Endpoint, n: usize) -> Result<Vec<u8>> {
        let mut buf: Vec<u8> = vec![0; n];

        let request_type = rusb::request_type(
            rusb::Direction::In,
            rusb::RequestType::Standard,
            rusb::Recipient::Device,
        );

        // TOOD: Check if we got the expected amount of data?
        self.handle
            .read_control(request_type, endpoint as u8, 0, 0, &mut buf, self.timeout)?;
        Ok(buf)
    }

    fn usb_write_control(&self, endpoint: Endpoint, value: u16, index: u16) -> Result<()> {
        let request_type = rusb::request_type(
            rusb::Direction::Out,
            rusb::RequestType::Standard,
            rusb::Recipient::Device,
        );
        self.handle.write_control(
            request_type,
            endpoint as u8,
            value,
            index,
            &[],
            self.timeout,
        )?;
        Ok(())
    }
}

impl CanAdapter for Panda {
    /// Sends a buffer of CAN messages to the panda.
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
        if frames.is_empty() {
            return Ok(());
        }

        let frames: Vec<Frame> = frames.drain(..).collect();
        let buf = usb_protocol::pack_can_buffer(&frames)?;

        for chunk in buf {
            self.handle
                .write_bulk(Endpoint::CanWrite as u8, &chunk, self.timeout)?;
        }
        Ok(())
    }

    /// Reads the current buffer of available CAN messages from the panda. This function will return an empty vector if no messages are available. In case of a recoverable error (e.g. unpacking error), the buffer will be cleared and an empty vector will be returned.
    fn recv(&mut self) -> Result<Vec<Frame>> {
        let mut buf: [u8; MAX_BULK_SIZE] = [0; MAX_BULK_SIZE];

        let recv: usize = self
            .handle
            .read_bulk(Endpoint::CanRead as u8, &mut buf, self.timeout)?;
        self.dat.extend_from_slice(&buf[0..recv]);

        let frames = usb_protocol::unpack_can_buffer(&mut self.dat);

        // Recover from unpacking errors, can_reset_communications() doesn't work properly
        match frames {
            Ok(frames) => Ok(frames),
            Err(e) => {
                warn!("Error unpacking: {:}", e);
                self.dat.clear();
                Ok(vec![])
            }
        }
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
