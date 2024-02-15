mod hw_type;
mod endpoint;

extern crate rusb;

use std;
use crate::error::Error;
use crate::panda::hw_type::HwType;
use crate::panda::endpoint::Endpoint;

static VENDOR_ID: u16 = 0xbbaa;
static PRODUCT_ID: u16 = 0xddcc;

pub struct Panda {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    timeout: std::time::Duration,
}

impl Panda {
    pub fn new() -> Result<Panda, Error>  {
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if device_desc.vendor_id() != VENDOR_ID {
                continue;
            }
            if device_desc.product_id() != PRODUCT_ID {
                continue;
            }

            return {
                Ok(Panda {
                    handle: device.open()?,
                    timeout: std::time::Duration::from_millis(100),
                })
            }
        }
        Err(Error::NotFound)
    }

    pub fn get_hw_type(&self) -> Result<HwType, Error> {
        let mut buf : [u8; 1] = [0];
        let request_type = rusb::request_type(rusb::Direction::In, rusb::RequestType::Standard, rusb::Recipient::Device);
        self.handle.read_control(request_type, Endpoint::HwType as u8, 0, 0, &mut buf, self.timeout)?;
        Ok(buf[0].into())
    }
}
