//! Contains the main error type for the library.
use thiserror::Error;

/// The main error type for the library. Each module has it's own error type that is contained by this error.
#[derive(Error, Debug, PartialEq, Copy, Clone)]
pub enum Error {
    #[error("Not Found")]
    NotFound,
    #[error("Not Supported")]
    NotSupported,
    #[error("Malformed Frame")]
    MalformedFrame,
    #[error("Timeout")]
    Timeout,
    #[error("Disconnected")]
    Disconnected,
    #[error(transparent)]
    IsoTPError(#[from] crate::isotp::Error),
    #[error(transparent)]
    LibUsbError(#[from] rusb::Error),
    #[error(transparent)]
    PandaError(#[from] crate::panda::Error),
    #[error(transparent)]
    UDSError(#[from] crate::uds::Error),
}

impl From<tokio_stream::Elapsed> for Error {
    fn from(_: tokio_stream::Elapsed) -> Error {
        Error::Timeout
    }
}
