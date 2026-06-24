//! Error types for the PEAK PCAN-USB FD adapter.
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Clone)]
pub enum Error {
    /// A frame had a payload length that is not a valid CAN(-FD) length.
    #[error("Malformed Frame")]
    MalformedFrame,
    /// The firmware-info response was shorter than expected.
    #[error("Short firmware info response")]
    ShortFirmwareInfo,
    /// A CAN-FD data bitrate was requested but none was configured.
    #[error("Missing CAN-FD data bitrate configuration")]
    MissingDataBitrate,
    /// A USB command transfer was only partially written to the device.
    #[error("Incomplete command write: {written} of {expected} bytes")]
    IncompleteWrite { written: usize, expected: usize },
}
