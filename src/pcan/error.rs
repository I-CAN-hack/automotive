//! PCAN adapter error types.

use std::ffi::{c_char, CStr};
use thiserror::Error;

/// PCAN-Basic status code wrapped with an English description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcanStatus {
    /// Raw `TPCANStatus` value returned by PCBUSB.
    pub code: u32,
    /// Human-readable description as returned by `CAN_GetErrorText`.
    pub message: String,
}

impl std::fmt::Display for PcanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:x} ({})", self.code, self.message)
    }
}

impl PcanStatus {
    /// Construct a [`PcanStatus`] from a raw PCAN-Basic status code, filling in
    /// the human-readable message via `CAN_GetErrorText`.
    pub fn from_code(code: u32) -> Self {
        let mut buf = [0i8; 256];
        let rc =
            unsafe { super::sys::CAN_GetErrorText(code, 0x09, buf.as_mut_ptr() as *mut c_char) };
        let message = if rc == super::sys::PCAN_ERROR_OK {
            unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };
        PcanStatus { code, message }
    }
}

/// Errors surfaced by the PCAN adapter.
#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("PCAN initialization failed on channel 0x{channel:02x}: {status}")]
    Initialize { channel: u16, status: PcanStatus },

    #[error("PCAN reports an unsupported bitrate configuration: {0}")]
    UnsupportedBitrate(String),

    #[error("PCAN adapter returned error: {0}")]
    Driver(PcanStatus),
}
