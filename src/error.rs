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
    #[error("Timeout")]
    Timeout,
    #[error("Disconnected")]
    Disconnected,

    #[cfg(not(target_arch = "wasm32"))]
    #[error(transparent)]
    IsoTPError(#[from] crate::isotp::Error),

    // Both `panda` and `peak` imply the `rusb-backend` feature.
    #[cfg(all(not(target_arch = "wasm32"), feature = "rusb-backend"))]
    #[error(transparent)]
    LibUsbError(#[from] rusb::Error),
    #[cfg(not(target_arch = "wasm32"))]
    #[error(transparent)]
    UDSError(#[from] crate::uds::Error),

    #[cfg(feature = "webusb")]
    #[error("WebUSB error: {0}")]
    WebUsbError(String),

    #[cfg(all(target_os = "windows", feature = "vector-xl"))]
    #[error(transparent)]
    VectorError(#[from] crate::vector::Error),

    #[cfg(all(target_os = "windows", feature = "vector-xl"))]
    #[error("Error loading DLL: {0}")]
    Libloading(std::sync::Arc<libloading::Error>),

    #[cfg(all(target_os = "windows", feature = "j2534"))]
    #[error(transparent)]
    J2534Error(#[from] crate::j2534::Error),

    #[cfg(feature = "panda")]
    #[error(transparent)]
    PandaError(#[from] crate::panda::Error),

    #[cfg(feature = "peak")]
    #[error(transparent)]
    PeakError(#[from] crate::peak::Error),
}

#[cfg(not(target_arch = "wasm32"))]
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
