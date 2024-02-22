//! Panda CAN adapter support

mod constants;
pub mod error;
mod usb_protocol;

use crate::async_can::AsyncCanAdapter;
use crate::can::CanAdapter;
use crate::error::Error;
use crate::panda::constants::{Endpoint, HwType, SafetyModel};
use tracing::{info, warn};

const VENDOR_ID: u16 = 0xbbaa;
const PRODUCT_ID: u16 = 0xddcc;
const EXPECTED_CAN_PACKET_VERSION: u8 = 4;


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
    pub fn new_async() -> Result<AsyncCanAdapter, Error> {
        let panda = Panda::new()?;
        Ok(AsyncCanAdapter::new(panda))
    }

    /// Connect to the first available panda. This function will set the safety mode to ALL_OUTPUT and clear all buffers.
    pub fn new() -> Result<Panda, Error> {
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if device_desc.vendor_id() != VENDOR_ID {
                continue;
            }
            if device_desc.product_id() != PRODUCT_ID {
                continue;
            }

            let mut panda = Panda {
                dat: vec![],
                handle: device.open()?,
                timeout: std::time::Duration::from_millis(100),
            };

            panda.handle.claim_interface(0)?;

            // Check panda firmware version
            let versions = panda.get_packets_versions()?;
            if versions.can_version != EXPECTED_CAN_PACKET_VERSION {
                return Err(Error::PandaError(
                    crate::panda::error::Error::WrongFirmwareVersion,
                ));
            }

            panda.set_safety_model(SafetyModel::AllOutput)?;
            panda.set_power_save(false)?;
            panda.set_heartbeat_disabled()?;
            panda.can_reset_communications()?;

            // can_reset_communications() doesn't work properly, flush manually
            panda.flush_rx()?;

            let hw_type = panda.get_hw_type()?;
            info!("Connected to Panda ({:?})", hw_type);

            return Ok(panda);
        }
        Err(Error::NotFound)
    }

    fn flush_rx(&self) -> Result<(), Error> {
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
    pub fn set_safety_model(&self, safety_model: SafetyModel) -> Result<(), Error> {
        let safety_param: u16 = 0;
        self.usb_write_control(Endpoint::SafetyModel, safety_model as u16, safety_param)
    }

    fn set_heartbeat_disabled(&self) -> Result<(), Error> {
        self.usb_write_control(Endpoint::HeartbeatDisabled, 0, 0)
    }

    fn set_power_save(&self, power_save_enabled: bool) -> Result<(), Error> {
        self.usb_write_control(Endpoint::PowerSave, power_save_enabled as u16, 0)
    }

    /// Get the hardware type of the panda. Usefull to detect if it supports CAN-FD.
    pub fn get_hw_type(&self) -> Result<HwType, Error> {
        let hw_type = self.usb_read_control(Endpoint::HwType, 1)?;
        HwType::from_repr(hw_type[0])
            .ok_or(Error::PandaError(crate::panda::error::Error::UnknownHwType))
    }

    fn get_packets_versions(&self) -> Result<Versions, Error> {
        let versions = self.usb_read_control(Endpoint::PacketsVersions, 3)?;
        Ok({
            Versions {
                health_version: versions[0],
                can_version: versions[1],
                can_health_version: versions[2],
            }
        })
    }

    fn can_reset_communications(&self) -> Result<(), Error> {
        self.usb_write_control(Endpoint::CanResetCommunications, 0, 0)
    }

    fn usb_read_control(&self, endpoint: Endpoint, n: usize) -> Result<Vec<u8>, Error> {
        let mut buf: Vec<u8> = Vec::with_capacity(n);
        buf.resize(n, 0);

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

    fn usb_write_control(&self, endpoint: Endpoint, value: u16, index: u16) -> Result<(), Error> {
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
            &mut [],
            self.timeout,
        )?;
        Ok(())
    }
}

impl CanAdapter for Panda {
    /// Sends a buffer of CAN messages to the panda.
    fn send(&mut self, frames: &[crate::can::Frame]) -> Result<(), Error> {
        if frames.is_empty() {
            return Ok(());
        }

        let buf = usb_protocol::pack_can_buffer(frames)?;

        // TODO: Handle buffer being too large

        self.handle
            .write_bulk(Endpoint::CanWrite as u8, &buf, self.timeout)?;
        Ok(())
    }

    /// Reads the current buffer of available CAN messages from the panda. This function will return an empty vector if no messages are available. In case of a recoverable error (e.g. unpacking error), the buffer will be cleared and an empty vector will be returned.
    fn recv(&mut self) -> Result<Vec<crate::can::Frame>, Error> {
        const N: usize = 16384;
        let mut buf: [u8; N] = [0; N];

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
