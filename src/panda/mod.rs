//! Panda CAN adapter support

mod constants;
mod error;
mod usb_protocol;

pub use error::Error;
use std::collections::VecDeque;

use crate::can::AsyncCanAdapter;
use crate::can::CanAdapter;
use crate::can::Frame;
use crate::panda::constants::{Endpoint, HwType, SafetyModel};
use crate::Result;
use tracing::{info, warn};

const USB_VIDS: &[u16] = &[0xbbaa, 0x3801];
const USB_PIDS: &[u16] = &[0xddee, 0xddcc];
const EXPECTED_CAN_PACKET_VERSION: u8 = 4;
const MAX_BULK_SIZE: usize = 16384;
const PANDA_BUS_CNT: usize = 3;

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
    pub fn new_async() -> Result<AsyncCanAdapter> {
        let panda = Panda::new()?;
        Ok(AsyncCanAdapter::new(panda))
    }

    /// Connect to the first available panda. This function will set the safety mode to ALL_OUTPUT and clear all buffers.
    pub fn new() -> Result<Panda> {
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

            panda.set_safety_model(SafetyModel::AllOutput)?;
            panda.set_power_save(false)?;
            panda.set_heartbeat_disabled()?;
            panda.can_reset_communications()?;

            for i in 0..PANDA_BUS_CNT {
                panda.set_canfd_auto(i, false)?;
            }

            // can_reset_communications() doesn't work properly, flush manually
            panda.flush_rx()?;

            let hw_type = panda.get_hw_type()?;
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
}
