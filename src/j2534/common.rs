//! Shared J2534 04.04 PassThru wire types, function-pointer signatures,
//! and helper utilities used by both the raw CAN adapter and the native
//! ISO 15765 transport.

use crate::can::Identifier;

// ── PASSTHRU_MSG ───────────────────────────────────────────────────────────

/// Identical layout to `PASSTHRU_MSG` in the SAE J2534 04.04 specification.
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

// ── SCONFIG / SCONFIG_LIST ─────────────────────────────────────────────────

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

// ── Constants ──────────────────────────────────────────────────────────────

pub const STATUS_NOERROR: i32 = 0x00;
pub const ERR_BUFFER_EMPTY: i32 = 0x10;
pub const ERR_TIMEOUT: i32 = 0x09;

/// `PassThruIoctl` IOCTL ID for writing channel configuration.
pub const IOCTL_SET_CONFIG: u32 = 0x02;

// ── Helper ─────────────────────────────────────────────────────────────────

/// Call `PassThruIoctl(SET_CONFIG)` with a single `(parameter, value)` pair.
///
/// Returns the raw J2534 status code.
pub fn set_config(
    ioctl_fn: FnPassThruIoctl,
    channel_id: u32,
    parameter: u32,
    value: u32,
) -> i32 {
    let mut cfg = SConfig { parameter, value };
    let mut list = SConfigList { num_of_params: 1, config_ptr: &mut cfg };
    unsafe {
        ioctl_fn(
            channel_id,
            IOCTL_SET_CONFIG,
            &mut list as *mut SConfigList as *mut _,
            std::ptr::null_mut(),
        )
    }
}

// ── Function-pointer signatures ────────────────────────────────────────────

pub type FnPassThruOpen =
    unsafe extern "system" fn(*const u8, *mut u32) -> i32;
pub type FnPassThruClose =
    unsafe extern "system" fn(u32) -> i32;
pub type FnPassThruConnect =
    unsafe extern "system" fn(u32, u32, u32, u32, *mut u32) -> i32;
pub type FnPassThruDisconnect =
    unsafe extern "system" fn(u32) -> i32;
pub type FnPassThruReadMsgs =
    unsafe extern "system" fn(u32, *mut PassThruMsg, *mut u32, u32) -> i32;
pub type FnPassThruWriteMsgs =
    unsafe extern "system" fn(u32, *mut PassThruMsg, *mut u32, u32) -> i32;
pub type FnPassThruStartMsgFilter =
    unsafe extern "system" fn(
        u32,
        u32,
        *const PassThruMsg,
        *const PassThruMsg,
        *const PassThruMsg,
        *mut u32,
    ) -> i32;
pub type FnPassThruIoctl =
    unsafe extern "system" fn(u32, u32, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;

// ── J2534 device handle ───────────────────────────────────────────────────

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
        let ret = unsafe { (self.close)(self.device_id) };
        tracing::trace!(ret = status_str(ret), "PassThruClose");
    }
}

/// Load a J2534 DLL, resolve all function pointers, and call `PassThruOpen`.
///
/// Pass `None` for `dll_path` to auto-discover the first 64-bit PassThru
/// driver from the Windows registry.
pub fn open_device(dll_path: Option<&str>) -> Result<J2534Device, String> {
    let path = resolve_dll_path(dll_path)?;

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

    let pass_thru_open   = sym!(b"PassThruOpen\0",           FnPassThruOpen);
    let close            = sym!(b"PassThruClose\0",          FnPassThruClose);
    let connect          = sym!(b"PassThruConnect\0",        FnPassThruConnect);
    let disconnect       = sym!(b"PassThruDisconnect\0",     FnPassThruDisconnect);
    let read             = sym!(b"PassThruReadMsgs\0",       FnPassThruReadMsgs);
    let write            = sym!(b"PassThruWriteMsgs\0",      FnPassThruWriteMsgs);
    let filter           = sym!(b"PassThruStartMsgFilter\0", FnPassThruStartMsgFilter);
    let ioctl            = sym!(b"PassThruIoctl\0",          FnPassThruIoctl);

    let mut device_id: u32 = 0;
    let ret = unsafe { pass_thru_open(std::ptr::null(), &mut device_id) };
    tracing::debug!(ret = status_str(ret), device_id, "PassThruOpen");
    if ret != STATUS_NOERROR {
        return Err(format!(
            "PassThruOpen failed: 0x{ret:02X} ({})",
            status_str(ret)
        ));
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

// ── DLL path resolution ────────────────────────────────────────────────────

/// Resolve the PassThru DLL path.
///
/// If `dll_path` is `Some`, uses it directly (after architecture check).
/// If `None`, discovers the first 64-bit driver from the Windows registry.
pub fn resolve_dll_path(dll_path: Option<&str>) -> Result<String, String> {
    let path = if let Some(p) = dll_path {
        p.to_owned()
    } else {
        let (native, wow32) = enumerate_passthru_drivers()
            .map_err(|e| format!("Cannot enumerate J2534 drivers: {e}"))?;

        if let Some(p) = native.into_iter().next() {
            p
        } else if wow32.is_empty() {
            return Err(
                "No J2534 PassThru drivers found in \
                 HKLM\\SOFTWARE\\PassThruSupport.04.04"
                    .to_owned(),
            );
        } else {
            return Err(format!(
                "No 64-bit J2534 drivers found. \
                 The following device(s) have 32-bit-only drivers registered \
                 under HKLM\\SOFTWARE\\WOW6432Node\\PassThruSupport.04.04, \
                 which cannot be loaded by this 64-bit process:\n  {}\n\
                 Options:\n  \
                   1. Install 64-bit drivers for your device (check manufacturer's website).\n  \
                   2. Use `j2534:<path>` to specify a 64-bit DLL explicitly.\n  \
                   3. Use a 32-bit build instead.",
                wow32.join("\n  ")
            ));
        }
    };

    check_dll_architecture(&path)?;
    Ok(path)
}

/// Returns `(native_64bit_paths, wow32_paths)`.
fn enumerate_passthru_drivers() -> Result<(Vec<String>, Vec<String>), String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    const PASSTHRU_KEY: &str = "SOFTWARE\\PassThruSupport.04.04";
    const PASSTHRU_KEY_WOW: &str = "SOFTWARE\\WOW6432Node\\PassThruSupport.04.04";

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    let native = read_passthru_paths(&hklm, PASSTHRU_KEY).unwrap_or_default();
    let wow32 = read_passthru_paths(&hklm, PASSTHRU_KEY_WOW)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !native.contains(p))
        .collect();

    Ok((native, wow32))
}

fn read_passthru_paths(hklm: &winreg::RegKey, key: &str) -> Result<Vec<String>, String> {
    use winreg::enums::KEY_READ;

    let root = hklm
        .open_subkey_with_flags(key, KEY_READ)
        .map_err(|e| e.to_string())?;

    let paths = root
        .enum_keys()
        .flatten()
        .filter_map(|name| {
            root.open_subkey_with_flags(&name, KEY_READ)
                .ok()
                .and_then(|sub| sub.get_value::<String, _>("FunctionLibrary").ok())
        })
        .collect();
    Ok(paths)
}

// ── PE header architecture check ──────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
enum DllMachine {
    X86,
    X64,
    Arm64,
    Other(u16),
}

fn dll_machine(path: &str) -> std::io::Result<DllMachine> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path)?;

    let mut magic = [0u8; 2];
    f.read_exact(&mut magic)?;
    if magic != [b'M', b'Z'] {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a PE file (no MZ header)",
        ));
    }

    f.seek(SeekFrom::Start(0x3C))?;
    let mut pe_offset_bytes = [0u8; 4];
    f.read_exact(&mut pe_offset_bytes)?;
    let pe_offset = u32::from_le_bytes(pe_offset_bytes) as u64;

    f.seek(SeekFrom::Start(pe_offset))?;
    let mut sig = [0u8; 4];
    f.read_exact(&mut sig)?;
    if sig != [b'P', b'E', 0, 0] {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a valid PE file (bad PE signature)",
        ));
    }

    let mut machine_bytes = [0u8; 2];
    f.read_exact(&mut machine_bytes)?;
    let machine = u16::from_le_bytes(machine_bytes);

    Ok(match machine {
        0x014C => DllMachine::X86,
        0x8664 => DllMachine::X64,
        0xAA64 => DllMachine::Arm64,
        other => DllMachine::Other(other),
    })
}

fn check_dll_architecture(path: &str) -> Result<(), String> {
    let machine = match dll_machine(path) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };

    #[cfg(target_arch = "x86_64")]
    if machine == DllMachine::X86 {
        return Err(format!(
            "J2534 DLL '{path}' is 32-bit (IMAGE_FILE_MACHINE_I386) and cannot \
             be loaded by this 64-bit process.\n\
             Options:\n  \
               1. Install 64-bit drivers for your device.\n  \
               2. Use a 32-bit build instead."
        ));
    }

    #[cfg(target_arch = "x86")]
    if machine == DllMachine::X64 {
        return Err(format!(
            "J2534 DLL '{path}' is 64-bit (IMAGE_FILE_MACHINE_AMD64) and cannot \
             be loaded by this 32-bit process.\n\
             Options:\n  \
               1. Install 32-bit drivers for your device.\n  \
               2. Use a 64-bit build instead."
        ));
    }

    Ok(())
}

// ── Diagnostics ────────────────────────────────────────────────────────────

pub fn status_str(ret: i32) -> &'static str {
    match ret {
        0x00 => "STATUS_NOERROR",
        0x01 => "ERR_NOT_SUPPORTED",
        0x02 => "ERR_INVALID_CHANNEL_ID",
        0x03 => "ERR_INVALID_PROTOCOL_ID",
        0x04 => "ERR_NULL_PARAMETER",
        0x05 => "ERR_INVALID_IOCTL_VALUE",
        0x06 => "ERR_INVALID_FLAGS",
        0x07 => "ERR_FAILED",
        0x08 => "ERR_DEVICE_NOT_CONNECTED",
        0x09 => "ERR_TIMEOUT",
        0x0A => "ERR_INVALID_MSG",
        0x0B => "ERR_INVALID_TIME_INTERVAL",
        0x0C => "ERR_EXCEEDED_LIMIT",
        0x0D => "ERR_INVALID_MSG_ID",
        0x0E => "ERR_DEVICE_IN_USE",
        0x0F => "ERR_INVALID_IOCTL_ID",
        0x10 => "ERR_BUFFER_EMPTY",
        0x11 => "ERR_BUFFER_FULL",
        0x12 => "ERR_BUFFER_OVERFLOW",
        0x13 => "ERR_PIN_INVALID",
        0x14 => "ERR_CHANNEL_IN_USE",
        0x15 => "ERR_MSG_PROTOCOL_ID",
        0x16 => "ERR_INVALID_FILTER_ID",
        0x17 => "ERR_NO_FLOW_CONTROL",
        0x18 => "ERR_NOT_UNIQUE",
        0x19 => "ERR_INVALID_BAUDRATE",
        0x1A => "ERR_INVALID_DEVICE_ID",
        _    => "ERR_UNKNOWN",
    }
}
