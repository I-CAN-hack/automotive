//! Contains the main error type for the library.

use thiserror::Error;

/// The main error type for the library. Each module has it's own error type that is contained by this error.
#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("Not Found")]
    NotFound,
    #[error("Not Supported")]
    NotSupported,
    #[error("Malformed Frame")]
    MalformedFrame,
    #[error("Invalid bitrate: {0}")]
    InvalidBitrate(String),
    #[error("Invalid adapter configuration: {0}")]
    InvalidAdapterConfig(String),
    #[error("Timeout")]
    Timeout,
    /// No message reception started before the transport's idle deadline.
    /// Polling the same receive stream again resumes waiting for a new message.
    /// A timeout during transmission or reassembly is [`Self::Timeout`] instead.
    #[error("Idle receive timeout")]
    IdleTimeout,
    #[error("Transmission cancelled by a transmit queue clear")]
    Cancelled,
    #[error("Disconnected")]
    Disconnected,

    #[error(transparent)]
    IoError(std::sync::Arc<std::io::Error>),

    #[error(transparent)]
    IsoTPError(#[from] crate::transports::isotp::Error),

    #[cfg(any(feature = "panda", feature = "peak"))]
    #[error(transparent)]
    LibUsbError(#[from] rusb::Error),
    #[error(transparent)]
    UDSError(#[from] crate::protocols::uds::Error),

    #[error(transparent)]
    XCPError(#[from] crate::protocols::xcp::Error),

    #[error(transparent)]
    XCPTransportError(#[from] crate::protocols::xcp::transport::Error),

    #[cfg(all(target_os = "windows", feature = "vector-xl"))]
    #[error(transparent)]
    VectorError(#[from] crate::adapters::vector::Error),

    #[cfg(all(target_os = "windows", feature = "vector-xl"))]
    #[error("Error loading DLL: {0}")]
    Libloading(std::sync::Arc<libloading::Error>),

    #[cfg(all(target_os = "windows", feature = "j2534"))]
    #[error(transparent)]
    J2534Error(#[from] crate::adapters::j2534::Error),

    #[cfg(feature = "panda")]
    #[error(transparent)]
    PandaError(#[from] crate::adapters::panda::Error),

    #[cfg(feature = "peak")]
    #[error(transparent)]
    PeakError(#[from] crate::adapters::peak::Error),
}

impl From<crate::can::bitrate::BitrateError> for Error {
    fn from(error: crate::can::bitrate::BitrateError) -> Self {
        Self::InvalidBitrate(error.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::IoError(std::sync::Arc::new(error))
    }
}

impl From<tokio_stream::Elapsed> for Error {
    fn from(_: tokio_stream::Elapsed) -> Error {
        Error::Timeout
    }
}

#[cfg(all(target_os = "windows", feature = "vector-xl"))]
impl From<libloading::Error> for Error {
    fn from(val: libloading::Error) -> Self {
        Self::Libloading(std::sync::Arc::new(val))
    }
}
