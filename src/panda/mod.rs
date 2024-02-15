mod endpoint;
mod hw_type;
mod safety_model;

extern crate rusb;

use crate::error::Error;
use crate::panda::endpoint::Endpoint;
use crate::panda::hw_type::HwType;
use crate::panda::safety_model::SafetyModel;
use std;

static VENDOR_ID: u16 = 0xbbaa;
static PRODUCT_ID: u16 = 0xddcc;

pub struct Panda {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    timeout: std::time::Duration,
}

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
                handle: device.open()?,
                timeout: std::time::Duration::from_millis(100),
            };
            panda.set_safety_model(SafetyModel::AllOutput)?;

            return Ok(panda);
        }
        Err(Error::NotFound)
    }

    fn usb_write(&self, endpoint: Endpoint, value: u16, index: u16) -> Result<(), Error> {
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

    pub fn set_safety_model(&self, safety_model: SafetyModel) -> Result<(), Error> {
        let safety_param: u16 = 0;
        self.usb_write(Endpoint::SafetyModel, safety_model as u16, safety_param)
    }

    pub fn get_hw_type(&self) -> Result<HwType, Error> {
        let mut buf: [u8; 1] = [0];
        let request_type = rusb::request_type(
            rusb::Direction::In,
            rusb::RequestType::Standard,
            rusb::Recipient::Device,
        );
        self.handle.read_control(
            request_type,
            Endpoint::HwType as u8,
            0,
            0,
            &mut buf,
            self.timeout,
        )?;
        Ok(buf[0].into())
    }
}
