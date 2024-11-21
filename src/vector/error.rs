//! Error types for the Vector Client.
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum Error {
    #[error("Driver error: {0}")]
    DriverError(String),
}
