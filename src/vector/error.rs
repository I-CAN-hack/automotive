//! Error types for the Vector Client.
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum Error {
    #[error("Driver error: {0}")]
    DriverError(String),
    #[error("BitTimming error: {0}")]
    BitTimingError(String),
    #[error("Qeue is empty")]
    EmptyQueue,
    #[error("Qeue is full")]
    FullQueue,
}
