//! Raw FFI bindings for the PCBUSB / PCAN-Basic API.
//!
//! Only the subset needed by the PCAN [`CanAdapter`](crate::can::CanAdapter)
//! implementation is declared. The wire format and constants are taken from
//! PEAK-System's public `PCANBasic.h` / UV Software's `PCBUSB.h`.

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::{c_char, c_void};

pub(crate) type TPCANHandle = u16;
pub(crate) type TPCANStatus = u32;

// PCAN channel handles we iterate over for auto-discovery.
pub(crate) const PCAN_USBBUS1: TPCANHandle = 0x51;
pub(crate) const PCAN_USBBUS_LAST: TPCANHandle = 0x58;

// PCAN-Basic status flags (subset).
pub(crate) const PCAN_ERROR_OK: TPCANStatus = 0x00000;
pub(crate) const PCAN_ERROR_QRCVEMPTY: TPCANStatus = 0x00020;
pub(crate) const PCAN_ERROR_QXMTFULL: TPCANStatus = 0x00080;
pub(crate) const PCAN_ERROR_BUSOFF: TPCANStatus = 0x00010;
pub(crate) const PCAN_ERROR_BUSHEAVY: TPCANStatus = 0x00008;
pub(crate) const PCAN_ERROR_BUSLIGHT: TPCANStatus = 0x00004;
pub(crate) const PCAN_ERROR_BUSPASSIVE: TPCANStatus = 0x40000;

// PCAN parameter IDs used by `CAN_SetValue`.
pub(crate) const PCAN_ALLOW_STATUS_FRAMES: u8 = 0x1E;
pub(crate) const PCAN_ALLOW_RTR_FRAMES: u8 = 0x1F;
pub(crate) const PCAN_ALLOW_ERROR_FRAMES: u8 = 0x20;
pub(crate) const PCAN_ALLOW_ECHO_FRAMES: u8 = 0x2C;

pub(crate) const PCAN_PARAMETER_OFF: u32 = 0x00;
pub(crate) const PCAN_PARAMETER_ON: u32 = 0x01;

// PCAN message type bitmask values.
pub(crate) const PCAN_MESSAGE_STANDARD: u8 = 0x00;
pub(crate) const PCAN_MESSAGE_RTR: u8 = 0x01;
pub(crate) const PCAN_MESSAGE_EXTENDED: u8 = 0x02;
pub(crate) const PCAN_MESSAGE_FD: u8 = 0x04;
pub(crate) const PCAN_MESSAGE_BRS: u8 = 0x08;
pub(crate) const PCAN_MESSAGE_ECHO: u8 = 0x20;
pub(crate) const PCAN_MESSAGE_ERRFRAME: u8 = 0x40;
pub(crate) const PCAN_MESSAGE_STATUS: u8 = 0x80;

// Classic CAN BTR0BTR1 constants (SJA1000 @ 16 MHz, as documented by PEAK).
pub(crate) const PCAN_BAUD_1M: u16 = 0x0014;
pub(crate) const PCAN_BAUD_800K: u16 = 0x0016;
pub(crate) const PCAN_BAUD_500K: u16 = 0x001C;
pub(crate) const PCAN_BAUD_250K: u16 = 0x011C;
pub(crate) const PCAN_BAUD_125K: u16 = 0x031C;
pub(crate) const PCAN_BAUD_100K: u16 = 0x432F;
pub(crate) const PCAN_BAUD_50K: u16 = 0x472F;
pub(crate) const PCAN_BAUD_20K: u16 = 0x532F;
pub(crate) const PCAN_BAUD_10K: u16 = 0x672F;

/// CAN-FD capable message layout.
///
/// Matches `tagTPCANMsgFD`: `DWORD` identifier, two `BYTE`s, then 64 `BYTE`s of
/// payload. The C compiler pads the struct to a 4-byte boundary, which Rust's
/// `repr(C)` reproduces.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct TPCANMsgFD {
    pub id: u32,
    pub msg_type: u8,
    pub dlc: u8,
    pub data: [u8; 64],
}

impl TPCANMsgFD {
    pub(crate) fn zeroed() -> Self {
        TPCANMsgFD {
            id: 0,
            msg_type: 0,
            dlc: 0,
            data: [0u8; 64],
        }
    }
}

#[link(name = "PCBUSB")]
extern "C" {
    pub(crate) fn CAN_Initialize(
        channel: TPCANHandle,
        btr0_btr1: u16,
        hw_type: u8,
        io_port: u32,
        interrupt: u16,
    ) -> TPCANStatus;

    pub(crate) fn CAN_InitializeFD(channel: TPCANHandle, bitrate_fd: *const c_char) -> TPCANStatus;

    pub(crate) fn CAN_Uninitialize(channel: TPCANHandle) -> TPCANStatus;

    pub(crate) fn CAN_GetStatus(channel: TPCANHandle) -> TPCANStatus;

    pub(crate) fn CAN_ReadFD(
        channel: TPCANHandle,
        msg: *mut TPCANMsgFD,
        timestamp: *mut u64,
    ) -> TPCANStatus;

    pub(crate) fn CAN_WriteFD(channel: TPCANHandle, msg: *const TPCANMsgFD) -> TPCANStatus;

    pub(crate) fn CAN_SetValue(
        channel: TPCANHandle,
        parameter: u8,
        buffer: *mut c_void,
        buffer_length: u32,
    ) -> TPCANStatus;

    pub(crate) fn CAN_GetErrorText(
        error: TPCANStatus,
        language: u16,
        buffer: *mut c_char,
    ) -> TPCANStatus;
}
