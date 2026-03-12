//! Shared J2534 04.04 PassThru wire types, function-pointer signatures,
//! and helper utilities used by both the raw CAN adapter and the native
//! ISO 15765 transport.

use super::constants::Status;
use crate::can::Identifier;

/// `PASSTHRU_MSG` from the SAE J2534 04.04 specification.
///
/// `repr(C)` with natural alignment.  Because every field before `data` is a
/// `u32` at a 4-byte-aligned offset the binary layout is the same as
/// `repr(C, packed(1))`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PassThruMsg {
    pub protocol_id: u32,
    pub rx_status: u32,
    pub tx_flags: u32,
    pub timestamp: u32,
    pub data_size: u32,
    pub extra_data_index: u32,
    pub data: [u8; 4128],
}

impl Default for PassThruMsg {
    fn default() -> Self {
        // SAFETY: all-zero bytes are valid for this POD struct.
        unsafe { std::mem::zeroed() }
    }
}

/// Single parameter entry passed to `PassThruIoctl(SET_CONFIG, …)`.
#[repr(C)]
pub struct SConfig {
    pub parameter: u32,
    pub value: u32,
}

/// Header block for `PassThruIoctl(SET_CONFIG, …)`.
#[repr(C)]
pub struct SConfigList {
    pub num_of_params: u32,
    pub config_ptr: *mut SConfig,
}

pub type FnPassThruOpen = unsafe extern "system" fn(*const u8, *mut u32) -> i32;
pub type FnPassThruClose = unsafe extern "system" fn(u32) -> i32;
pub type FnPassThruConnect = unsafe extern "system" fn(u32, u32, u32, u32, *mut u32) -> i32;
pub type FnPassThruDisconnect = unsafe extern "system" fn(u32) -> i32;
pub type FnPassThruReadMsgs =
    unsafe extern "system" fn(u32, *mut PassThruMsg, *mut u32, u32) -> i32;
pub type FnPassThruWriteMsgs =
    unsafe extern "system" fn(u32, *mut PassThruMsg, *mut u32, u32) -> i32;
pub type FnPassThruStartMsgFilter = unsafe extern "system" fn(
    u32,
    u32,
    *const PassThruMsg,
    *const PassThruMsg,
    *const PassThruMsg,
    *mut u32,
) -> i32;
pub type FnPassThruIoctl =
    unsafe extern "system" fn(u32, u32, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;

/// Owns a J2534 device (the `PassThruOpen` handle) and all resolved DLL
/// function pointers.  On [`Drop`], calls `PassThruClose` to release the
/// device.
///
/// This struct enables opening multiple channels on the same device without
/// closing and reopening the physical connection between channels.
pub struct J2534Device {
    pub(crate) device_id: u32,
    pub(crate) close: FnPassThruClose,
    pub(crate) connect: FnPassThruConnect,
    pub(crate) disconnect: FnPassThruDisconnect,
    pub(crate) read: FnPassThruReadMsgs,
    pub(crate) write: FnPassThruWriteMsgs,
    pub(crate) filter: FnPassThruStartMsgFilter,
    pub(crate) ioctl: FnPassThruIoctl,
    /// Keeps the DLL loaded for the lifetime of this device.
    pub(crate) _lib: libloading::Library,
}

impl Drop for J2534Device {
    fn drop(&mut self) {
        let status = Status::from(unsafe { (self.close)(self.device_id) });
        tracing::trace!(ret = %status, "PassThruClose");
    }
}

/// Bit 31 flag in the 4-byte CAN ID field of a `PassThruMsg`, indicating a
/// 29-bit extended CAN identifier.
pub const CAN_29BIT_ID: u32 = 0x8000_0000;

impl PassThruMsg {
    /// Build a message carrying `payload` after a 4-byte big-endian CAN ID.
    /// Used for both `PROTOCOL_CAN` and `PROTOCOL_ISO15765` frames.
    ///
    /// For [`Identifier::Extended`], bit 31 (`CAN_29BIT_ID`) is set in the
    /// ID field as required by the J2534 specification.
    pub fn new(protocol_id: u32, id: Identifier, payload: &[u8]) -> Self {
        let raw_id: u32 = match id {
            Identifier::Extended(v) => v | CAN_29BIT_ID,
            Identifier::Standard(v) => v,
        };
        let id_bytes = raw_id.to_be_bytes();
        let mut data = [0u8; 4128];
        data[..4].copy_from_slice(&id_bytes);
        data[4..4 + payload.len()].copy_from_slice(payload);
        let data_size = (4 + payload.len()) as u32;
        Self {
            protocol_id,
            data,
            data_size,
            extra_data_index: data_size,
            ..Default::default()
        }
    }

    /// Build a message from a raw `u32` CAN ID (no extended-ID flag logic).
    ///
    /// Useful for filter masks (e.g. `0xFFFF_FFFF`) where the value is not a
    /// real CAN arbitration ID.
    pub fn new_raw(protocol_id: u32, raw_can_id: u32, payload: &[u8]) -> Self {
        let id_bytes = raw_can_id.to_be_bytes();
        let mut data = [0u8; 4128];
        data[..4].copy_from_slice(&id_bytes);
        data[4..4 + payload.len()].copy_from_slice(payload);
        let data_size = (4 + payload.len()) as u32;
        Self {
            protocol_id,
            data,
            data_size,
            extra_data_index: data_size,
            ..Default::default()
        }
    }
}

/// Parse a 4-byte big-endian CAN ID from a `PassThruMsg` data field.
///
/// If bit 31 (`CAN_29BIT_ID`) is set, returns [`Identifier::Extended`] with
/// the lower 29 bits.  Otherwise returns [`Identifier::Standard`].
pub fn parse_can_id(data: &[u8]) -> Identifier {
    let raw = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if raw & CAN_29BIT_ID != 0 {
        Identifier::Extended(raw & !CAN_29BIT_ID)
    } else {
        Identifier::Standard(raw)
    }
}

/// Call `PassThruIoctl(SET_CONFIG)` with a single `(parameter, value)` pair.
pub fn set_config(
    ioctl_fn: FnPassThruIoctl,
    channel_id: u32,
    parameter: super::constants::IoctlParam,
    value: u32,
) -> Status {
    let mut cfg = SConfig {
        parameter: parameter.into(),
        value,
    };
    let mut list = SConfigList {
        num_of_params: 1,
        config_ptr: &mut cfg,
    };
    let ret = unsafe {
        ioctl_fn(
            channel_id,
            super::constants::IoctlId::SetConfig.into(),
            &mut list as *mut SConfigList as *mut _,
            std::ptr::null_mut(),
        )
    };
    Status::from(ret)
}

/// Load a J2534 DLL, resolve all function pointers, and call `PassThruOpen`.
///
/// Pass `None` for `dll_path` to auto-discover the first 64-bit PassThru
/// driver from the Windows registry.
pub fn open_device(dll_path: Option<&str>) -> Result<J2534Device, String> {
    let path = super::dll::resolve_dll_path(dll_path)?;

    let lib = match unsafe { libloading::Library::new(&path) } {
        Ok(l) => l,
        Err(e) => return Err(format!("Cannot load {path}: {e}")),
    };

    macro_rules! sym {
        ($name:literal, $ty:ty) => {
            match unsafe { lib.get::<$ty>($name) } {
                Ok(s) => *s,
                Err(e) => {
                    return Err(format!(
                        "Symbol {} not found in {path}: {e}",
                        std::str::from_utf8($name).unwrap_or("?")
                    ));
                }
            }
        };
    }

    let pass_thru_open = sym!(b"PassThruOpen\0", FnPassThruOpen);
    let close = sym!(b"PassThruClose\0", FnPassThruClose);
    let connect = sym!(b"PassThruConnect\0", FnPassThruConnect);
    let disconnect = sym!(b"PassThruDisconnect\0", FnPassThruDisconnect);
    let read = sym!(b"PassThruReadMsgs\0", FnPassThruReadMsgs);
    let write = sym!(b"PassThruWriteMsgs\0", FnPassThruWriteMsgs);
    let filter = sym!(b"PassThruStartMsgFilter\0", FnPassThruStartMsgFilter);
    let ioctl = sym!(b"PassThruIoctl\0", FnPassThruIoctl);

    let mut device_id: u32 = 0;
    let status = Status::from(unsafe { pass_thru_open(std::ptr::null(), &mut device_id) });
    tracing::debug!(ret = %status, device_id, "PassThruOpen");
    if status != Status::NoError {
        return Err(format!("PassThruOpen failed: {status}"));
    }

    Ok(J2534Device {
        device_id,
        close,
        connect,
        disconnect,
        read,
        write,
        filter,
        ioctl,
        _lib: lib,
    })
}
