//! Contains the main error type for the library.
use thiserror::Error;

/// The main error type for the library. Each module has it's own error type that is contained by this error.
#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("Not Found")]
    NotFound,
    #[error("Malformed Frame")]
    MalformedFrame,
    #[error("Timeout")]
    Timeout,
    #[error(transparent)]
    IsoTPError(crate::isotp::error::Error),
    #[error(transparent)]
    LibUsbError(#[from] rusb::Error),
    #[error(transparent)]
    PandaError(crate::panda::error::Error),
    #[error(transparent)]
    UDSError(crate::uds::error::Error),
}

impl From<tokio_stream::Elapsed> for Error {
    fn from(_: tokio_stream::Elapsed) -> Error {
        Error::Timeout
    }
}
