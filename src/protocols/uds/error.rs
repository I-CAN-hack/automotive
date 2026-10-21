//! Error types for the UDS Client.
use thiserror::Error;

/// Negative Response Codes returned by ECU as defined in ISO 14229
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum NegativeResponseCode {
    GeneralReject = 0x10,
    ServiceNotSupported = 0x11,
    SubFunctionNotSupported = 0x12,
    IncorrectMessageLengthOrInvalidFormat = 0x13,
    ResponseTooLong = 0x14,
    BusyRepeatRequest = 0x21,
    ConditionsNotCorrect = 0x22,
    RequestSequenceError = 0x24,
    NoResponseFromSubnetComponent = 0x25,
    FailurePreventsExecutionOfRequestedAction = 0x26,
    RequestOutOfRange = 0x31,
    SecurityAccessDenied = 0x33,
    InvalidKey = 0x35,
    ExeedNumberOfAttempts = 0x36,
    RequiredTimeDelayNotExpired = 0x37,
    UploadDownloadNotAccepted = 0x70,
    TransferDataSuspended = 0x71,
    GeneralProgrammingFailure = 0x72,
    WrongBlockSequenceCounter = 0x73,
    RequestCorrectlyReceivedResponsePending = 0x78,
    SubFunctionNotSupportedInActiveSession = 0x7e,
    ServiceNotSupportedInActiveSession = 0x7f,

    NonStandard(u8),
}

impl From<u8> for NegativeResponseCode {
    fn from(val: u8) -> NegativeResponseCode {
        match val {
            0x10 => NegativeResponseCode::GeneralReject,
            0x11 => NegativeResponseCode::ServiceNotSupported,
            0x12 => NegativeResponseCode::SubFunctionNotSupported,
            0x13 => NegativeResponseCode::IncorrectMessageLengthOrInvalidFormat,
            0x14 => NegativeResponseCode::ResponseTooLong,
            0x21 => NegativeResponseCode::BusyRepeatRequest,
            0x22 => NegativeResponseCode::ConditionsNotCorrect,
            0x24 => NegativeResponseCode::RequestSequenceError,
            0x25 => NegativeResponseCode::NoResponseFromSubnetComponent,
            0x26 => NegativeResponseCode::FailurePreventsExecutionOfRequestedAction,
            0x31 => NegativeResponseCode::RequestOutOfRange,
            0x33 => NegativeResponseCode::SecurityAccessDenied,
            0x35 => NegativeResponseCode::InvalidKey,
            0x36 => NegativeResponseCode::ExeedNumberOfAttempts,
            0x37 => NegativeResponseCode::RequiredTimeDelayNotExpired,
            0x70 => NegativeResponseCode::UploadDownloadNotAccepted,
            0x71 => NegativeResponseCode::TransferDataSuspended,
            0x72 => NegativeResponseCode::GeneralProgrammingFailure,
            0x73 => NegativeResponseCode::WrongBlockSequenceCounter,
            0x78 => NegativeResponseCode::RequestCorrectlyReceivedResponsePending,
            0x7e => NegativeResponseCode::SubFunctionNotSupportedInActiveSession,
            0x7f => NegativeResponseCode::ServiceNotSupportedInActiveSession,
            _ => NegativeResponseCode::NonStandard(val),
        }
    }
}

/// An existing protocol requirement enforced by strict mode.
#[derive(Error, Debug, PartialEq, Clone)]
pub enum ProtocolDeviation {
    #[error("Expected response service ID {expected:#04x}, received {actual:#04x}")]
    InvalidServiceId { expected: u8, actual: u8 },
    #[error("Expected sub-function {expected:#04x}, received {actual:#04x}")]
    InvalidSubFunction { expected: u8, actual: u8 },
    #[error("Expected data identifier {expected:#06x}, received {actual:#06x}")]
    InvalidDataIdentifier { expected: u16, actual: u16 },
    #[error("Expected routine identifier {expected:#06x}, received {actual:#06x}")]
    InvalidRoutineIdentifier { expected: u16, actual: u16 },
    #[error("Expected block sequence counter {expected:#04x}, received {actual:#04x}")]
    InvalidBlockSequenceCounter { expected: u8, actual: u8 },
    #[error("Invalid response length (minimum {minimum}, maximum {maximum:?})")]
    InvalidResponseLength {
        minimum: usize,
        maximum: Option<usize>,
    },
    #[error("Expected 1 to 8 block-length bytes, received a length format declaring {0}")]
    InvalidLengthFormat(usize),
}

#[derive(Error, Debug, PartialEq, Clone)]
pub enum Error {
    /// Strict-mode failure with the complete received UDS PDU, including SID
    /// and any echoed sub-function or identifier bytes.
    #[error("Protocol deviation: {deviation}; response: {response:02x?}")]
    ProtocolDeviation {
        deviation: ProtocolDeviation,
        response: Vec<u8>,
    },
    #[error("Invalid Reponse Service ID: {0}")]
    InvalidServiceId(u8),
    #[error("Invalid Response Sub Function ID: {0}")]
    InvalidSubFunction(u8),
    #[error("Invalid Response Data Identifer: {0}")]
    InvalidDataIdentifier(u16),
    #[error("Invalid Response Routine Identifer: {0}")]
    InvalidRoutineIdentifier(u16),
    #[error("Invalid Block Sequence Counter: {0}")]
    InvalidBlockSequenceCounter(u8),
    #[error("Invalid Response Length")]
    InvalidResponseLength,
    #[error("Negative Response: {0:?}")]
    NegativeResponse(NegativeResponseCode),
}
