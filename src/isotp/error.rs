//! Error types for the ISO-TP protocol.

use std::fmt;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    DataTooLarge,
    FlowControl,
    OutOfOrder,
    UnknownFrameType,
    MalformedFrame,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::DataTooLarge => write!(fmt, "Data Too Large"),
            Error::FlowControl => write!(fmt, "Flow Control"),
            Error::OutOfOrder => write!(fmt, "Out Of Order"),
            Error::UnknownFrameType => write!(fmt, "Unknown Frame Type"),
            Error::MalformedFrame => write!(fmt, "Malformed Frame"),
        }
    }
}

impl std::error::Error for Error {}
