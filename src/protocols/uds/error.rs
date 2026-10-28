//! Error types for the UDS Client.
use thiserror::Error;

use super::{DataIdentifier, RoutineIdentifier, ServiceIdentifier};

wire_code_enum! {
    /// Negative Response Codes returned by ECU as defined in ISO 14229
    pub enum NegativeResponseCode: u8 {
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
    }
}

/// A protocol requirement enforced by strict mode.
#[derive(Error, Debug, PartialEq, Clone)]
pub enum ProtocolDeviation {
    #[error("Expected response service ID {expected:#04x}, received {actual:#04x}")]
    InvalidServiceId {
        expected: ServiceIdentifier,
        actual: ServiceIdentifier,
    },
    #[error("Expected echoed request service ID {expected:#04x}, received {actual:#04x}")]
    InvalidRequestServiceId {
        expected: ServiceIdentifier,
        actual: ServiceIdentifier,
    },
    #[error("Expected sub-function {expected:#04x}, received {actual:#04x}")]
    InvalidSubFunction { expected: u8, actual: u8 },
    #[error("Received a suppressed positive response without a preceding NRC 0x78")]
    UnexpectedPositiveResponse,
    #[error("Expected data identifier {expected:#06x}, received {actual:#06x}")]
    InvalidDataIdentifier {
        expected: DataIdentifier,
        actual: DataIdentifier,
    },
    #[error("Expected routine identifier {expected:#06x}, received {actual:#06x}")]
    InvalidRoutineIdentifier {
        expected: RoutineIdentifier,
        actual: RoutineIdentifier,
    },
    #[error("Expected block sequence counter {expected:#04x}, received {actual:#04x}")]
    InvalidBlockSequenceCounter { expected: u8, actual: u8 },
    #[error("Invalid response length (minimum {minimum}, maximum {maximum:?})")]
    InvalidResponseLength {
        minimum: usize,
        maximum: Option<usize>,
    },
    #[error("Expected 1 to 15 block-length bytes, received a length format declaring {0}")]
    InvalidLengthFormat(usize),
    #[error("Length format identifier {0:#04x} must have a zero low nibble")]
    InvalidLengthFormatIdentifier(u8),
    #[error("Maximum block length must be at least 3 bytes, received {0}")]
    InvalidMaxNumberOfBlockLength(u128),
    #[error("Expected address and length format {expected:#04x}, received {actual:#04x}")]
    InvalidAddressAndLengthFormatIdentifier { expected: u8, actual: u8 },
    #[error("Expected memory address {expected:02x?}, received {actual:02x?}")]
    InvalidMemoryAddress { expected: Vec<u8>, actual: Vec<u8> },
    #[error("Expected memory size {expected:02x?}, received {actual:02x?}")]
    InvalidMemorySize { expected: Vec<u8>, actual: Vec<u8> },
    #[error("Expected {expected} memory-record bytes, received {actual}")]
    InvalidMemoryRecordLength { expected: u128, actual: usize },
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
    InvalidServiceId(ServiceIdentifier),
    #[error("Invalid Response Sub Function ID: {0}")]
    InvalidSubFunction(u8),
    #[error("Invalid Response Data Identifer: {0}")]
    InvalidDataIdentifier(DataIdentifier),
    #[error("Invalid Response Routine Identifer: {0}")]
    InvalidRoutineIdentifier(RoutineIdentifier),
    #[error("Invalid Block Sequence Counter: {0}")]
    InvalidBlockSequenceCounter(u8),
    #[error("Invalid Response Length")]
    InvalidResponseLength,
    #[error("Compression method {0:#04x} does not fit in a nibble")]
    InvalidCompressionMethod(u8),
    #[error("Encryption method {0:#04x} does not fit in a nibble")]
    InvalidEncryptionMethod(u8),
    #[error("Memory field width must be 1 to 8 bytes, received {0}")]
    InvalidMemoryValueWidth(usize),
    #[error("Memory value {value:#x} does not fit in {width} bytes")]
    MemoryValueOverflow { value: u64, width: usize },
    #[error("Block length {value} cannot be represented as usize; response: {response:02x?}")]
    BlockLengthOverflow { value: u128, response: Vec<u8> },
    #[error("Negative Response: {0:?}")]
    NegativeResponse(NegativeResponseCode),
}

// TryInto is infallible when a caller supplies an already-validated method.
impl From<std::convert::Infallible> for Error {
    fn from(value: std::convert::Infallible) -> Self {
        match value {}
    }
}
