use std::fmt;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    InvalidServiceId(u8),
    InvalidSubFunction(u8),
    InvalidDataIdentifier(u16),
    InvalidResponseLength,
    NegativeResponse(NegativeResponseCode),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::InvalidServiceId(id) => write!(fmt, "Invalid Response Service ID: {}", id),
            Error::InvalidSubFunction(id) => {
                write!(fmt, "Invalid Response Sub Function ID: {}", id)
            }
            Error::InvalidDataIdentifier(id) => {
                write!(fmt, "Invalid Response Data Identifer: {}", id)
            }
            Error::InvalidResponseLength => write!(fmt, "Invalid Response Length"),
            Error::NegativeResponse(e) => write!(fmt, "Negative Response: {:?}", e),
        }
    }
}
impl std::error::Error for Error {}
