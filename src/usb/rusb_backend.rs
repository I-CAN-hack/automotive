//! Native [`UsbBackend`] implementation backed by [`rusb`](https://crates.io/crates/rusb).

use std::time::Duration;

use crate::usb::UsbBackend;
use crate::Result;

/// USB backend using `rusb` (libusb). Each async method performs the equivalent blocking
/// libusb call and returns immediately, so it can be driven with a trivial `block_on`.
pub struct RusbBackend {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
}

impl RusbBackend {
    /// Scan for the first connected device matching any of `vids`/`pids`, open it, and
    /// claim interface 0. Returns [`crate::Error::NotFound`] if no matching device exists.
    pub fn open_first(vids: &[u16], pids: &[u16]) -> Result<Self> {
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if !vids.contains(&device_desc.vendor_id()) {
                continue;
            }
            if !pids.contains(&device_desc.product_id()) {
                continue;
            }

            let handle = device.open()?;
            handle.claim_interface(0)?;
            return Ok(RusbBackend { handle });
        }
        Err(crate::Error::NotFound)
    }
}

impl UsbBackend for RusbBackend {
    async fn read_bulk(
        &self,
        endpoint: u8,
        max_len: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; max_len];
        let n = self.handle.read_bulk(endpoint, &mut buf, timeout)?;
        buf.truncate(n);
        Ok(buf)
    }

    async fn write_bulk(&self, endpoint: u8, data: &[u8], timeout: Duration) -> Result<()> {
        self.handle.write_bulk(endpoint, data, timeout)?;
        Ok(())
    }

    async fn read_control(
        &self,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        let request_type = rusb::request_type(
            rusb::Direction::In,
            rusb::RequestType::Standard,
            rusb::Recipient::Device,
        );
        let mut buf = vec![0u8; len];
        let n = self
            .handle
            .read_control(request_type, request, value, index, &mut buf, timeout)?;
        buf.truncate(n);
        Ok(buf)
    }

    async fn write_control(
        &self,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<()> {
        let request_type = rusb::request_type(
            rusb::Direction::Out,
            rusb::RequestType::Standard,
            rusb::Recipient::Device,
        );
        self.handle
            .write_control(request_type, request, value, index, data, timeout)?;
        Ok(())
    }
}
