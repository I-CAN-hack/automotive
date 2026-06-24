//! Generic USB backend abstraction.
//!
//! This trait abstracts the small set of USB transfer operations that USB-based CAN
//! adapters (currently the [`crate::panda::Panda`]) need. It allows swapping the
//! underlying USB implementation, e.g. [`rusb`](https://crates.io/crates/rusb) on native
//! platforms, or WebUSB when targeting the browser (`wasm32`).
//!
//! The trait is asynchronous because WebUSB is Promise-based. On native platforms the
//! [`RusbBackend`] implementation simply performs the equivalent blocking `rusb` call and
//! returns immediately, so it can be driven with a trivial `block_on` from the existing
//! blocking [`crate::can::CanAdapter`] path.
//!
//! Direction is implied by the method: `read_*` performs an IN transfer, `write_*`
//! performs an OUT transfer. Control transfers use the Standard request type and Device
//! recipient, which is all the panda protocol requires.

use std::time::Duration;

use crate::Result;

#[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
mod rusb_backend;
#[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
pub use rusb_backend::RusbBackend;

#[cfg(all(target_arch = "wasm32", feature = "webusb"))]
mod webusb;
#[cfg(all(target_arch = "wasm32", feature = "webusb"))]
pub use webusb::WebUsbBackend;

/// Asynchronous USB backend used by USB-based CAN adapters.
///
/// The `read_*`/`write_*` methods map directly onto libusb-style bulk and control
/// transfers. `endpoint` keeps the libusb convention (the direction bit is set for IN
/// endpoints, e.g. `0x81`); WebUSB backends mask it off (`endpoint & 0x7f`).
#[allow(async_fn_in_trait)]
pub trait UsbBackend {
    /// Perform a bulk IN transfer from `endpoint`, returning up to `max_len` bytes.
    async fn read_bulk(&self, endpoint: u8, max_len: usize, timeout: Duration)
        -> Result<Vec<u8>>;

    /// Perform a bulk OUT transfer of `data` to `endpoint`.
    async fn write_bulk(&self, endpoint: u8, data: &[u8], timeout: Duration) -> Result<()>;

    /// Perform a Standard/Device control IN transfer, returning up to `len` bytes.
    /// `request` is the `bRequest` value.
    async fn read_control(
        &self,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>>;

    /// Perform a Standard/Device control OUT transfer. `request` is the `bRequest` value.
    async fn write_control(
        &self,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<()>;
}
