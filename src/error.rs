//! Contains the main error type for the library.
use std::fmt;

/// The main error type for the library. Each module has it's own error type that is contained by this error.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    NotFound,
    MalformedFrame,
    Timeout,
    IsoTPError(crate::isotp::error::Error),
    LibUsbError(rusb::Error),
    PandaError(crate::panda::error::Error),
    UDSError(crate::uds::error::Error),
}

impl From<rusb::Error> for Error {
    fn from(err: rusb::Error) -> Error {
        Error::LibUsbError(err)
    }
}

impl From<tokio_stream::Elapsed> for Error {
    fn from(_: tokio_stream::Elapsed) -> Error {
        Error::Timeout
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::NotFound => write!(fmt, "Not Found"),
            Error::MalformedFrame => write!(fmt, "Malformed Frame"),
            Error::Timeout => write!(fmt, "Timeout"),
            Error::IsoTPError(err) => err.fmt(fmt),
            Error::LibUsbError(err) => err.fmt(fmt),
            Error::PandaError(err) => err.fmt(fmt),
            Error::UDSError(err) => err.fmt(fmt),
        }
    }
}

impl std::error::Error for Error {}
