use std::fmt;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    DataTooLarge,
    FlowControl,
    OutOfOrder,
    UnknownFrameType,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::DataTooLarge => write!(fmt, "Data Too Large"),
            Error::FlowControl => write!(fmt, "Flow Control"),
            Error::OutOfOrder => write!(fmt, "Out Of Order"),
            Error::UnknownFrameType => write!(fmt, "Unknown Frame Type"),
        }
    }
}

impl std::error::Error for Error {}
