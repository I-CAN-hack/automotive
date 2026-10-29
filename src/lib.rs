#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod adapters;
pub mod can;
mod error;
pub mod protocols;
mod shared;
pub mod transports;
pub mod utils;

/// Re-export of relevant stream traits from `tokio_stream`.
pub use tokio_stream::{Stream, StreamExt, Timeout};

pub use error::Error;
pub use transports::Transport;
pub type Result<T> = std::result::Result<T, Error>;
