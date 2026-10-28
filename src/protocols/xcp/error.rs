use thiserror::Error;

use super::Command;

wire_code_enum! {
    /// XCP command error codes; unknown ECU codes preserve every wire bit.
    pub enum ErrorCode: u8 {
        CommandSynchronization = 0x00,
        CommandBusy = 0x10,
        DaqActive = 0x11,
        PgmActive = 0x12,
        CommandUnknown = 0x20,
        CommandSyntax = 0x21,
        OutOfRange = 0x22,
        WriteProtected = 0x23,
        AccessDenied = 0x24,
        AccessLocked = 0x25,
        PageNotValid = 0x26,
        ModeNotValid = 0x27,
        SegmentNotValid = 0x28,
        Sequence = 0x29,
        DaqConfig = 0x2a,
        MemoryOverflow = 0x30,
        Generic = 0x31,
        Verify = 0x32,
        ResourceTemporarilyNotAccessible = 0x33,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error("Invalid XCP argument: {0}")]
    InvalidArgument(String),
    #[error("XCP CONNECT is required before this command")]
    NotConnected,
    #[error("XCP command {command:#04x} was rejected with error {code:#04x}")]
    NegativeResponse {
        command: Command,
        code: ErrorCode,
        data: Vec<u8>,
    },
    #[error("Malformed XCP response: {reason}; packet: {response:02x?}")]
    MalformedResponse { reason: String, response: Vec<u8> },
    #[error("XCP command outcome is uncertain; call synchronize before sending another command")]
    SynchronizationRequired,
}

pub(crate) fn malformed(reason: impl Into<String>, response: &[u8]) -> crate::Error {
    Error::MalformedResponse {
        reason: reason.into(),
        response: response.to_vec(),
    }
    .into()
}
