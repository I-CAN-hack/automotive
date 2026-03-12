//! Shared J2534 04.04 PassThru wire types, function-pointer signatures,
//! and helper utilities used by both the raw CAN adapter and the native
//! ISO 15765 transport.

use super::constants::{FilterType, IoctlId, IoctlParam, Protocol, Status, CAN_29BIT_ID_FLAG};
use super::error::Error as J2534Error;
use crate::can::Identifier;
use crate::Result;

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

/// Connected J2534 channel plus the callback set needed to operate it.
///
/// The callbacks are copied out of [`J2534Device`] so adapter implementations
/// can keep using them after channel setup without repeatedly unpacking the
/// device handle.
#[derive(Clone, Copy)]
pub(crate) struct J2534Channel {
    pub(crate) channel_id: u32,
    pub(crate) disconnect: FnPassThruDisconnect,
    pub(crate) read: FnPassThruReadMsgs,
    pub(crate) write: FnPassThruWriteMsgs,
    pub(crate) filter: FnPassThruStartMsgFilter,
    pub(crate) ioctl: FnPassThruIoctl,
}

impl J2534Channel {
    pub(crate) fn disconnect(&self) -> Status {
        Status::from(unsafe { (self.disconnect)(self.channel_id) })
    }

    pub(crate) fn clear_rx_buffer(&self) -> Status {
        Status::from(unsafe {
            (self.ioctl)(
                self.channel_id,
                IoctlId::ClearRxBuffer.into(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        })
    }

    pub(crate) fn read_message(&self, msg: &mut PassThruMsg, timeout_ms: u32) -> (Status, u32) {
        let mut count: u32 = 1;
        let status =
            Status::from(unsafe { (self.read)(self.channel_id, msg, &mut count, timeout_ms) });
        (status, count)
    }

    pub(crate) fn write_message(&self, msg: &mut PassThruMsg, timeout_ms: u32) -> (Status, u32) {
        let mut count: u32 = 1;
        let status =
            Status::from(unsafe { (self.write)(self.channel_id, msg, &mut count, timeout_ms) });
        (status, count)
    }

    /// Call `PassThruIoctl(SET_CONFIG)` with a single `(parameter, value)` pair.
    pub(crate) fn set_config(&self, parameter: IoctlParam, value: u32) -> Status {
        let mut cfg = SConfig {
            parameter: parameter.into(),
            value,
        };
        let mut list = SConfigList {
            num_of_params: 1,
            config_ptr: &mut cfg,
        };
        let ret = unsafe {
            (self.ioctl)(
                self.channel_id,
                IoctlId::SetConfig.into(),
                &mut list as *mut SConfigList as *mut _,
                std::ptr::null_mut(),
            )
        };
        Status::from(ret)
    }

    /// Install a pass-all receive filter on a CAN channel.
    pub(crate) fn install_pass_all_can_filter(&self) -> Status {
        for msg in pass_all_can_filter_messages() {
            let (status, filter_id) = self.start_msg_filter(FilterType::Pass, &msg, &msg, None);
            tracing::debug!(
                ret = %status,
                filter_id,
                tx_flags = format_args!("0x{:04X}", msg.tx_flags),
                "PassThruStartMsgFilter"
            );
            if status != Status::NoError {
                return status;
            }
        }

        Status::NoError
    }

    /// Install the ISO 15765 flow-control filter used by the native ISO-TP channel.
    pub(crate) fn install_iso15765_flow_control_filter(
        &self,
        tx_id: Identifier,
        rx_id: Identifier,
        ext_address: Option<u8>,
        tx_flags: u32,
    ) -> Status {
        let proto: u32 = Protocol::Iso15765.into();
        let mut mask_msg = PassThruMsg::new_raw(proto, 0xFFFF_FFFF, &[]);
        if ext_address.is_some() {
            mask_msg.data[4] = 0xFF;
            mask_msg.data_size = 5;
            mask_msg.extra_data_index = 5;
        }
        let mut pattern_msg = PassThruMsg::new_with_ext_address(proto, rx_id, ext_address, &[]);
        let mut fc_msg = PassThruMsg::new_with_ext_address(proto, tx_id, ext_address, &[]);
        mask_msg.tx_flags = tx_flags;
        pattern_msg.tx_flags = tx_flags;
        fc_msg.tx_flags = tx_flags;

        let (status, filter_id) = self.start_msg_filter(
            FilterType::FlowControl,
            &mask_msg,
            &pattern_msg,
            Some(&fc_msg),
        );
        let tx_raw: u32 = tx_id.into();
        let rx_raw: u32 = rx_id.into();
        tracing::debug!(
            ret = %status,
            filter_id,
            tx_id = format_args!("{tx_raw:08X}"),
            rx_id = format_args!("{rx_raw:08X}"),
            ext_address,
            "PassThruStartMsgFilter (FLOW_CONTROL)"
        );
        status
    }

    fn start_msg_filter(
        &self,
        filter_type: FilterType,
        mask_msg: &PassThruMsg,
        pattern_msg: &PassThruMsg,
        flow_control_msg: Option<&PassThruMsg>,
    ) -> (Status, u32) {
        let mut filter_id: u32 = 0;
        let status = Status::from(unsafe {
            (self.filter)(
                self.channel_id,
                filter_type.into(),
                mask_msg,
                pattern_msg,
                flow_control_msg.map_or(std::ptr::null(), |msg| msg),
                &mut filter_id,
            )
        });
        (status, filter_id)
    }
}

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

/// Call `PassThruConnect` for `protocol` with the provided `flags`.
pub(crate) fn connect_channel_with_flags(
    device: &J2534Device,
    protocol: Protocol,
    flags: u32,
    bitrate: u32,
) -> Result<J2534Channel> {
    let mut channel_id: u32 = 0;
    let status = Status::from(unsafe {
        (device.connect)(
            device.device_id,
            protocol.into(),
            flags,
            bitrate,
            &mut channel_id,
        )
    });
    tracing::debug!(
        ret = %status,
        protocol = protocol_name(protocol),
        flags = format_args!("0x{flags:08X}"),
        channel_id,
        bitrate,
        "PassThruConnect"
    );
    if status != Status::NoError {
        return Err(J2534Error::DllError(format!(
            "PassThruConnect ({}, {bitrate} bps) failed: {status}",
            protocol_name(protocol)
        ))
        .into());
    }

    Ok(J2534Channel {
        channel_id,
        disconnect: device.disconnect,
        read: device.read,
        write: device.write,
        filter: device.filter,
        ioctl: device.ioctl,
    })
}

/// J2534 `TxFlags`/`RxStatus` bits derived from a CAN identifier.
pub fn can_id_flags(id: Identifier) -> u32 {
    if id.is_extended() {
        CAN_29BIT_ID_FLAG
    } else {
        0
    }
}

fn pass_all_can_filter_messages() -> [PassThruMsg; 2] {
    let standard = PassThruMsg::new_raw(Protocol::Can.into(), 0, &[]);
    let mut extended = standard;
    extended.tx_flags = CAN_29BIT_ID_FLAG;
    [standard, extended]
}

impl PassThruMsg {
    /// Build a message carrying `payload` after a 4-byte big-endian CAN ID.
    /// Used for both `PROTOCOL_CAN` and `PROTOCOL_ISO15765` frames.
    pub fn new(protocol_id: u32, id: Identifier, payload: &[u8]) -> Self {
        Self::new_with_ext_address(protocol_id, id, None, payload)
    }

    /// Build a message carrying an optional ISO 15765 extended address
    /// between the CAN ID and payload.
    pub fn new_with_ext_address(
        protocol_id: u32,
        id: Identifier,
        ext_address: Option<u8>,
        payload: &[u8],
    ) -> Self {
        let raw_id: u32 = id.into();
        Self::new_raw_with_ext_address(protocol_id, raw_id, ext_address, payload)
    }

    /// Build a message from a raw `u32` CAN ID (no extended-ID flag logic).
    ///
    /// Useful for filter masks (e.g. `0xFFFF_FFFF`) where the value is not a
    /// real CAN arbitration ID.
    pub fn new_raw(protocol_id: u32, raw_can_id: u32, payload: &[u8]) -> Self {
        Self::new_raw_with_ext_address(protocol_id, raw_can_id, None, payload)
    }

    /// Build a message from a raw `u32` CAN ID with an optional ISO 15765
    /// extended address byte before `payload`.
    pub fn new_raw_with_ext_address(
        protocol_id: u32,
        raw_can_id: u32,
        ext_address: Option<u8>,
        payload: &[u8],
    ) -> Self {
        let id_bytes = raw_can_id.to_be_bytes();
        let mut data = [0u8; 4128];
        data[..4].copy_from_slice(&id_bytes);
        let mut offset = 4;
        if let Some(ext_address) = ext_address {
            data[offset] = ext_address;
            offset += 1;
        }
        data[offset..offset + payload.len()].copy_from_slice(payload);
        let data_size = (offset + payload.len()) as u32;
        Self {
            protocol_id,
            data,
            data_size,
            extra_data_index: data_size,
            ..Default::default()
        }
    }
}

/// Parse a CAN identifier from a J2534 message.
pub fn parse_can_id(msg: &PassThruMsg) -> Identifier {
    let raw = u32::from_be_bytes([msg.data[0], msg.data[1], msg.data[2], msg.data[3]]);
    if msg.rx_status & CAN_29BIT_ID_FLAG != 0 {
        Identifier::Extended(raw)
    } else {
        assert!(
            raw <= 0x7FF,
            "J2534 message missing 29-bit flag for non-standard CAN ID 0x{raw:08X}"
        );
        Identifier::Standard(raw)
    }
}

/// Load a J2534 DLL, resolve all function pointers, and call `PassThruOpen`.
///
/// Pass `None` for `dll_path` to auto-discover the first 64-bit PassThru
/// driver from the Windows registry.
pub fn open_device(dll_path: Option<&str>) -> Result<J2534Device> {
    let path = super::dll::resolve_dll_path(dll_path)?;

    let lib = match unsafe { libloading::Library::new(&path) } {
        Ok(l) => l,
        Err(e) => return Err(J2534Error::DllError(format!("Cannot load {path}: {e}")).into()),
    };

    macro_rules! sym {
        ($name:literal, $ty:ty) => {
            match unsafe { lib.get::<$ty>($name) } {
                Ok(s) => *s,
                Err(e) => {
                    return Err(J2534Error::DllError(format!(
                        "Symbol {} not found in {path}: {e}",
                        std::str::from_utf8($name).unwrap_or("?")
                    ))
                    .into());
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
        return Err(J2534Error::DllError(format!("PassThruOpen failed: {status}")).into());
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

fn protocol_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Can => "CAN",
        Protocol::Iso15765 => "ISO15765",
    }
}
