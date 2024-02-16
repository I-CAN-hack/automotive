use std::{fmt, result};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    InvalidChecksum,
    WrongFirmwareVersion,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        fmt.write_str(match self {
            Error::InvalidChecksum => "Invalid Checksum",
            Error::WrongFirmwareVersion => "Wrong Firmware Version",
        })
    }
}

impl std::error::Error for Error {}
