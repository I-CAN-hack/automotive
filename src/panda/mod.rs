pub mod error;
mod constants;
mod usb_protocol;

use crate::can::CanAdapter;
use crate::error::Error;
use crate::panda::constants::{Endpoint, HwType, SafetyModel};
use tracing::{info, warn};

const VENDOR_ID: u16 = 0xbbaa;
const PRODUCT_ID: u16 = 0xddcc;
const EXPECTED_CAN_PACKET_VERSION: u8 = 4;

pub struct Panda {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    timeout: std::time::Duration,
    dat: Vec<u8>,
}

pub struct Versions {
    pub health_version: u8,
    pub can_version: u8,
    pub can_health_version: u8,
}

unsafe impl Send for Panda {}

impl Panda {
    pub fn new() -> Result<Panda, Error> {
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if device_desc.vendor_id() != VENDOR_ID {
                continue;
            }
            if device_desc.product_id() != PRODUCT_ID {
                continue;
            }

            let panda = Panda {
                dat: vec![],
                handle: device.open()?,
                timeout: std::time::Duration::from_millis(100),
            };

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

            info!("Connected to panda");

            return Ok(panda);
        }
        Err(Error::NotFound)
    }

    pub fn set_safety_model(&self, safety_model: SafetyModel) -> Result<(), Error> {
        let safety_param: u16 = 0;
        self.usb_write_control(Endpoint::SafetyModel, safety_model as u16, safety_param)
    }

    pub fn set_heartbeat_disabled(&self) -> Result<(), Error> {
        self.usb_write_control(Endpoint::HeartbeatDisabled, 0, 0)
    }

    pub fn set_power_save(&self, power_save_enabled: bool) -> Result<(), Error> {
        self.usb_write_control(Endpoint::PowerSave, power_save_enabled as u16, 0)
    }

    pub fn get_hw_type(&self) -> Result<HwType, Error> {
        let hw_type = self.usb_read_control(Endpoint::HwType, 1)?;
        Ok(hw_type[0].into())
    }

    pub fn get_packets_versions(&self) -> Result<Versions, Error> {
        let versions = self.usb_read_control(Endpoint::PacketsVersions, 3)?;
        Ok({
            Versions {
                health_version: versions[0],
                can_version: versions[1],
                can_health_version: versions[2],
            }
        })
    }

    pub fn can_reset_communications(&self) -> Result<(), Error> {
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

impl Drop for Panda {
    fn drop(&mut self) {
        info!("Closing panda");
    }
}

impl CanAdapter for Panda {
    fn send(&mut self, frames: &[crate::can::Frame]) -> Result<(), Error> {
        if frames.is_empty() {
            return Ok(());
        }

        let buf = usb_protocol::pack_can_buffer(frames)?;
        self.handle
            .write_bulk(Endpoint::CanWrite as u8, &buf, self.timeout)?;
        Ok(())
    }

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
