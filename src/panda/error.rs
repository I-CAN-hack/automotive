//! Error types for the Panda
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Copy, Clone)]
pub enum Error {
    /// CAN Packet has invalid checksum in the header
    #[error("Invalid Checksum")]
    InvalidChecksum,
    /// Panda firmware version doesn't match the expected version
    #[error("Wrong Firmware Version")]
    WrongFirmwareVersion,
    /// Unexpected hardware type
    #[error("Unknown Hardware Type")]
    UnknownHwType,
}
