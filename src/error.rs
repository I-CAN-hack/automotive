use std::{fmt, result};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    NotFound,
    MalformedFrame,
    Timeout,
    PandaError(crate::panda::error::Error),
    LibUsbError(rusb::Error),
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
    fn fmt(&self, fmt: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match self {
            Error::LibUsbError(err) => err.fmt(fmt),
            Error::PandaError(err) => err.fmt(fmt),
            _ => fmt.write_str(match self {
                Error::NotFound => "Not found",
                Error::Timeout => "Timeout",
                Error::MalformedFrame => "Malformed Frame",
                Error::PandaError(_) => unreachable!(),
                Error::LibUsbError(_) => unreachable!(),
            }),
        }
    }
}

impl std::error::Error for Error {}
