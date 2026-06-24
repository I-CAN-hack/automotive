//! Native [`UsbBackend`] implementation backed by [`rusb`](https://crates.io/crates/rusb).

use std::time::Duration;

use crate::usb::{ControlType, Recipient, UsbBackend};
use crate::Result;

/// USB backend using `rusb` (libusb). Each async method performs the equivalent blocking
/// libusb call and returns immediately, so it can be driven with a trivial `block_on`.
pub struct RusbBackend {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
}

impl RusbBackend {
    /// Scan for the first connected device matching any of `vids`/`pids`, open it, detach
    /// any in-kernel driver, and claim interface 0. Returns [`crate::Error::NotFound`] if
    /// no matching device exists.
    pub fn open_first(vids: &[u16], pids: &[u16]) -> Result<Self> {
        for device in rusb::devices()?.iter() {
            let device_desc = match device.device_descriptor() {
                Ok(desc) => desc,
                Err(_) => continue,
            };

            if !vids.contains(&device_desc.vendor_id()) {
                continue;
            }
            if !pids.contains(&device_desc.product_id()) {
                continue;
            }

            let handle = device.open()?;
            // Detach the in-kernel driver (e.g. `peak_usb`) if bound, so we can claim the
            // interface for raw access. A no-op for devices with no kernel driver.
            handle.set_auto_detach_kernel_driver(true).ok();
            handle.claim_interface(0)?;
            return Ok(RusbBackend { handle });
        }
        Err(crate::Error::NotFound)
    }
}

fn request_type(dir: rusb::Direction, ctrl_type: ControlType, recipient: Recipient) -> u8 {
    let ty = match ctrl_type {
        ControlType::Standard => rusb::RequestType::Standard,
        ControlType::Class => rusb::RequestType::Class,
        ControlType::Vendor => rusb::RequestType::Vendor,
    };
    let recip = match recipient {
        Recipient::Device => rusb::Recipient::Device,
        Recipient::Interface => rusb::Recipient::Interface,
        Recipient::Endpoint => rusb::Recipient::Endpoint,
        Recipient::Other => rusb::Recipient::Other,
    };
    rusb::request_type(dir, ty, recip)
}

impl UsbBackend for RusbBackend {
    async fn read_bulk(
        &self,
        endpoint: u8,
        max_len: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; max_len];
        match self.handle.read_bulk(endpoint, &mut buf, timeout) {
            Ok(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            Err(rusb::Error::Timeout) => Ok(vec![]),
            Err(rusb::Error::NoDevice) => Err(crate::Error::Disconnected),
            Err(e) => Err(e.into()),
        }
    }

    async fn write_bulk(&self, endpoint: u8, data: &[u8], timeout: Duration) -> Result<usize> {
        match self.handle.write_bulk(endpoint, data, timeout) {
            Ok(n) => Ok(n),
            Err(rusb::Error::Timeout) => Ok(0),
            Err(rusb::Error::NoDevice) => Err(crate::Error::Disconnected),
            Err(e) => Err(e.into()),
        }
    }

    async fn read_control(
        &self,
        ctrl_type: ControlType,
        recipient: Recipient,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        let rt = request_type(rusb::Direction::In, ctrl_type, recipient);
        let mut buf = vec![0u8; len];
        match self
            .handle
            .read_control(rt, request, value, index, &mut buf, timeout)
        {
            Ok(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            Err(rusb::Error::Timeout) => Ok(vec![]),
            Err(rusb::Error::NoDevice) => Err(crate::Error::Disconnected),
            Err(e) => Err(e.into()),
        }
    }

    async fn write_control(
        &self,
        ctrl_type: ControlType,
        recipient: Recipient,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<()> {
        let rt = request_type(rusb::Direction::Out, ctrl_type, recipient);
        match self
            .handle
            .write_control(rt, request, value, index, data, timeout)
        {
            Ok(_) => Ok(()),
            Err(rusb::Error::NoDevice) => Err(crate::Error::Disconnected),
            Err(e) => Err(e.into()),
        }
    }
}
