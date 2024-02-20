use std::{fmt, result};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    DataTooLarge,
    FlowControl,
    OutOfOrder,
    UnknownFrameType,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        fmt.write_str(match self {
            Error::DataTooLarge => "Data Too Large",
            Error::FlowControl => "Flow Control",
            Error::OutOfOrder => "Out Of Order",
            Error::UnknownFrameType => "Unknown Frame Type",
        })
    }
}

impl std::error::Error for Error {}
