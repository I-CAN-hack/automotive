//! Browser [`UsbBackend`] implementation backed by the [WebUSB API].
//!
//! [WebUSB API]: https://developer.mozilla.org/en-US/docs/Web/API/WebUSB_API

use std::time::Duration;

use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{UsbControlTransferParameters, UsbDevice, UsbInTransferResult, UsbOutTransferResult};

use crate::usb::{ControlType, Recipient, UsbBackend};
use crate::Result;

/// USB backend using the browser's WebUSB API. Holds an opened, claimed [`UsbDevice`].
///
/// WebUSB transfers have no timeout, so the `timeout` arguments are ignored: bulk reads
/// resolve when the device sends data (the adapter's processing loop awaits them).
pub struct WebUsbBackend {
    device: UsbDevice,
}

impl WebUsbBackend {
    /// Prompt the user to pick a device matching any of `vids` (via `navigator.usb`), then
    /// open it and claim interface 0. Must be called from a user gesture.
    ///
    /// Note: WebUSB cannot detach an in-kernel driver. On Linux a device bound to a kernel
    /// driver (e.g. PEAK's `peak_usb`) must be unbound before it can be claimed here.
    pub async fn request(vids: &[u16], _pids: &[u16]) -> Result<Self> {
        let usb = web_sys::window()
            .ok_or_else(|| err("no window object (WebUSB requires a browser context)"))?
            .navigator()
            .usb();

        // Build `{ filters: [{ vendorId }, ...] }`.
        let filters = Array::new();
        for vid in vids {
            let filter = Object::new();
            set(&filter, "vendorId", JsValue::from(*vid))?;
            filters.push(&filter);
        }
        let options = Object::new();
        set(&options, "filters", filters.into())?;

        let device: UsbDevice = JsFuture::from(usb.request_device(&options.unchecked_into()))
            .await
            .map_err(js_err)?
            .unchecked_into();

        JsFuture::from(device.open()).await.map_err(js_err)?;
        JsFuture::from(device.select_configuration(1))
            .await
            .map_err(js_err)?;
        JsFuture::from(device.claim_interface(0))
            .await
            .map_err(js_err)?;

        Ok(WebUsbBackend { device })
    }
}

impl UsbBackend for WebUsbBackend {
    async fn read_bulk(
        &self,
        endpoint: u8,
        max_len: usize,
        _timeout: Duration,
    ) -> Result<Vec<u8>> {
        let result: UsbInTransferResult =
            JsFuture::from(self.device.transfer_in(endpoint & 0x7f, max_len as u32))
                .await
                .map_err(js_err)?
                .unchecked_into();
        Ok(in_result_to_vec(&result))
    }

    async fn write_bulk(&self, endpoint: u8, data: &[u8], _timeout: Duration) -> Result<usize> {
        // The binding takes `&mut [u8]`, so a temporary copy of the caller's data is needed.
        let mut buf = data.to_vec();
        let promise = self
            .device
            .transfer_out_with_u8_slice(endpoint & 0x7f, &mut buf)
            .map_err(js_err)?;
        let result: UsbOutTransferResult =
            JsFuture::from(promise).await.map_err(js_err)?.unchecked_into();
        Ok(result.bytes_written() as usize)
    }

    async fn read_control(
        &self,
        ctrl_type: ControlType,
        recipient: Recipient,
        request: u8,
        value: u16,
        index: u16,
        len: usize,
        _timeout: Duration,
    ) -> Result<Vec<u8>> {
        let setup = control_setup(ctrl_type, recipient, request, value, index)?;
        let result: UsbInTransferResult =
            JsFuture::from(self.device.control_transfer_in(&setup, len as u16))
                .await
                .map_err(js_err)?
                .unchecked_into();
        Ok(in_result_to_vec(&result))
    }

    async fn write_control(
        &self,
        ctrl_type: ControlType,
        recipient: Recipient,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        _timeout: Duration,
    ) -> Result<()> {
        let setup = control_setup(ctrl_type, recipient, request, value, index)?;
        if data.is_empty() {
            JsFuture::from(self.device.control_transfer_out(&setup))
                .await
                .map_err(js_err)?;
        } else {
            let mut buf = data.to_vec();
            let promise = self
                .device
                .control_transfer_out_with_u8_slice(&setup, &mut buf)
                .map_err(js_err)?;
            JsFuture::from(promise).await.map_err(js_err)?;
        }
        Ok(())
    }
}

/// Build `UsbControlTransferParameters` from the request type and recipient.
fn control_setup(
    ctrl_type: ControlType,
    recipient: Recipient,
    request: u8,
    value: u16,
    index: u16,
) -> Result<UsbControlTransferParameters> {
    // WebUSB forbids `standard`-type control transfers carrying non-standard `bRequest`
    // values ("transfer not allowed"). Devices that dispatch on `bRequest` alone (panda,
    // PEAK) accept them as vendor requests, so issue Standard as Vendor.
    let request_type = match ctrl_type {
        ControlType::Standard | ControlType::Vendor => "vendor",
        ControlType::Class => "class",
    };
    let recipient = match recipient {
        Recipient::Device => "device",
        Recipient::Interface => "interface",
        Recipient::Endpoint => "endpoint",
        Recipient::Other => "other",
    };

    let setup = Object::new();
    set(&setup, "requestType", JsValue::from_str(request_type))?;
    set(&setup, "recipient", JsValue::from_str(recipient))?;
    set(&setup, "request", JsValue::from(request))?;
    set(&setup, "value", JsValue::from(value))?;
    set(&setup, "index", JsValue::from(index))?;
    Ok(setup.unchecked_into())
}

/// Extract the bytes from a transfer result's `DataView` (empty if absent).
fn in_result_to_vec(result: &UsbInTransferResult) -> Vec<u8> {
    match result.data() {
        Some(view) => {
            let array = Uint8Array::new_with_byte_offset_and_length(
                &view.buffer(),
                view.byte_offset() as u32,
                view.byte_length() as u32,
            );
            array.to_vec()
        }
        None => vec![],
    }
}

fn set(obj: &Object, key: &str, value: JsValue) -> Result<()> {
    Reflect::set(obj, &JsValue::from_str(key), &value).map_err(js_err)?;
    Ok(())
}

fn err(msg: &str) -> crate::Error {
    crate::Error::WebUsbError(msg.to_string())
}

fn js_err(value: JsValue) -> crate::Error {
    let msg = value
        .as_string()
        .or_else(|| {
            value
                .dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
        })
        .unwrap_or_else(|| format!("{value:?}"));
    crate::Error::WebUsbError(msg)
}
