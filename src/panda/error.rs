//! Error types for the Panda

use std::fmt;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    /// CAN Packet has invalid checksum in the header
    InvalidChecksum,
    /// Panda firmware version doesn't match the expected version
    WrongFirmwareVersion,
    /// Unexpected hardware type
    UnknownHwType,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::InvalidChecksum => write!(fmt, "Invalid Checksum"),
            Error::WrongFirmwareVersion => write!(fmt, "Wrong Firmware Version"),
            Error::UnknownHwType => write!(fmt, "Wrong Firmware Version"),
        }
    }
}

impl std::error::Error for Error {}
