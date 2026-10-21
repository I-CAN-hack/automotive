#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod can;
mod error;
pub mod isotp;
pub mod uds;

/// Re-export of relevant stream traits from `tokio_stream`.
pub use tokio_stream::{Stream, StreamExt, Timeout};

pub use error::Error;
pub use isotp::TransportLayer;
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(all(target_os = "linux", feature = "socketcan"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "linux", feature = "socketcan"))))]
pub mod socketcan;

#[cfg(all(target_os = "windows", feature = "vector-xl"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "windows", feature = "vector-xl"))))]
pub mod vector;

#[cfg(all(target_os = "windows", feature = "j2534"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "windows", feature = "j2534"))))]
pub mod j2534;

#[cfg(feature = "panda")]
#[cfg_attr(docsrs, doc(cfg(feature = "panda")))]
pub mod panda;

#[cfg(feature = "peak")]
#[cfg_attr(docsrs, doc(cfg(feature = "peak")))]
pub mod peak;
