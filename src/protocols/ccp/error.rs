use super::Command;

wire_code_enum! {
    /// CCP return codes; unknown ECU codes retain their wire values.
    pub enum ErrorCode: u8 {
        Acknowledge = 0x00,
        DaqOverload = 0x01,
        CommandBusy = 0x10,
        DaqBusy = 0x11,
        InternalTimeout = 0x12,
        KeyRequest = 0x18,
        SessionStatusRequest = 0x19,
        ColdStartRequest = 0x20,
        CalibrationDataInitRequest = 0x21,
        DaqListInitRequest = 0x22,
        CodeUpdateRequest = 0x23,
        CommandUnknown = 0x30,
        CommandSyntax = 0x31,
        OutOfRange = 0x32,
        AccessDenied = 0x33,
        Overload = 0x34,
        AccessLocked = 0x35,
        ResourceNotAvailable = 0x36,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid CCP argument: {0}")]
    InvalidArgument(String),
    #[error("CCP CONNECT is required before this command")]
    NotConnected,
    #[error("CCP command {command:#04x} was rejected with error {code:#04x}")]
    NegativeResponse {
        command: Command,
        code: ErrorCode,
        data: Vec<u8>,
    },
    #[error("CCP event {code:#04x} interrupted the command; reconnect before continuing")]
    Event { code: ErrorCode, data: Vec<u8> },
    #[error("Malformed CCP response: {response:02x?}; expected an eight-byte CRM")]
    MalformedResponse { response: Vec<u8> },
    #[error("CCP command outcome is uncertain; reconnect and re-establish MTA before continuing")]
    ReconnectRequired,
}
