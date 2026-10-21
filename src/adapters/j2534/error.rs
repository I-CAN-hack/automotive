//! Error types for the J2534 adapters.

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum Error {
    #[error("DLL error: {0}")]
    DllError(String),
}
