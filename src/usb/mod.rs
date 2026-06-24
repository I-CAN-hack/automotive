//! Generic USB backend abstraction.
//!
//! This trait abstracts the USB transfer operations that USB-based CAN adapters (the
//! [`crate::panda::Panda`] and [`crate::peak::Peak`]) need. It allows swapping the
//! underlying USB implementation, e.g. [`rusb`](https://crates.io/crates/rusb) on native
//! platforms, or WebUSB when targeting the browser (`wasm32`).
//!
//! The trait is asynchronous because WebUSB is Promise-based. On native platforms the
//! [`RusbBackend`] implementation simply performs the equivalent blocking `rusb` call and
//! returns immediately, so it can be driven with a trivial `block_on` from the existing
//! blocking [`crate::can::CanAdapter`] path.

// Control transfers inherently take type/recipient/request/value/index/len/timeout.
#![allow(clippy::too_many_arguments)]

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

/// Control-transfer request type (the `type` field of `bmRequestType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlType {
    Standard,
    Class,
    Vendor,
}

/// Control-transfer recipient (the `recipient` field of `bmRequestType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipient {
    Device,
    Interface,
    Endpoint,
    Other,
}

/// Asynchronous USB backend used by USB-based CAN adapters.
///
/// `endpoint` keeps the libusb convention (the direction bit is set for IN endpoints, e.g.
/// `0x81`); WebUSB backends mask it off (`endpoint & 0x7f`).
#[allow(async_fn_in_trait)]
pub trait UsbBackend {
    /// Perform a bulk IN transfer from `endpoint`, returning up to `max_len` bytes. A read
    /// that times out returns an empty vector (rather than an error) so callers can poll
    /// without special-casing each backend's timeout representation.
    async fn read_bulk(&self, endpoint: u8, max_len: usize, timeout: Duration)
        -> Result<Vec<u8>>;

    /// Perform a bulk OUT transfer of `data` to `endpoint`, returning the number of bytes
    /// actually written (which may be less than `data.len()` on a short write or timeout).
    async fn write_bulk(&self, endpoint: u8, data: &[u8], timeout: Duration) -> Result<usize>;

    /// Perform a control IN transfer, returning up to `len` bytes (empty on timeout).
    /// `request` is the `bRequest` value.
    async fn read_control(
        &self,
        ctrl_type: ControlType,
        recipient: Recipient,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>>;

    /// Perform a control OUT transfer. `request` is the `bRequest` value.
    async fn write_control(
        &self,
        ctrl_type: ControlType,
        recipient: Recipient,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> Result<()>;
}
