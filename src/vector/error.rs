//! Error types for the Vector Client.
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Copy, Clone)]
pub enum Error {
    #[error("Driver error: {0}")]
    DriverError(u8),
    // #[error("Invalid Response Sub Function ID: {0}")]
    // InvalidSubFunction(u8),
    // #[error("Invalid Response Data Identifer: {0}")]
    // InvalidDataIdentifier(u16),
    // #[error("Invalid Response Routine Identifer: {0}")]
    // InvalidRoutineIdentifier(u16),
    // #[error("Invalid Block Sequence Counter: {0}")]
    // InvalidBlockSequenceCounter(u8),
    // #[error("Invalid Response Length")]
    // InvalidResponseLength,
    // #[error("Negative Response: {0:?}")]
    // NegativeResponse(NegativeResponseCode),
}
