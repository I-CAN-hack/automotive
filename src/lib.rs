//! # The Automotive Crate
//! Welcome to the `automotive` crate documentation. The purpose of this crate is to help you with all things automotive related. Most importantly, it provides a fully async CAN interface supporting multiple adapters.
//!
//! ## Async CAN Example
//!
//! The following adapter opens the first available adapter on the system, and then receives all frames.
//!
//! ```rust
//! use futures_util::stream::StreamExt;
//! async fn can_example() {
//!     let adapter = automotive::can::get_adapter().unwrap();
//!     let mut stream = adapter.recv();
//!
//!     while let Some(frame) = stream.next().await {
//!         let id: u32 = frame.id.into();
//!         println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
//!     }
//! }
//! ```
//!
//! ## UDS Example
//!
//! The automotive crate also supplies interfaces for various diagnostic protocols such as UDS. The adapter is first wrapped to support the ISO Transport Layer, then a UDS Client is created. All methods are fully async, making it easy to communicate with multiple ECUs in parallel.
//!
//! ```rust
//! async fn uds_example() {
//!     let adapter = automotive::can::get_adapter().unwrap();
//!     let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
//!     let uds = automotive::uds::UDSClient::new(&isotp);
//!
//!     uds.tester_present().await.unwrap();
//!     let response = uds.read_data_by_identifier(automotive::uds::DataIdentifier::ApplicationSoftwareIdentification as u16).await.unwrap();
//!
//!     println!("Application Software Identification: {}", hex::encode(response));
//! }
//! ```
//!
//! ## Suported adapters
//!  - SocketCAN (Linux only, supported using [socketcan-rs](https://github.com/socketcan-rs/socketcan-rs))
//!  - comma.ai panda (all platforms)
//!

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub mod can;
mod error;
pub mod isotp;
pub mod panda;
pub mod uds;

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(target_os = "linux")]
pub mod socketcan;

// #[cfg(target_os = "windows")]
pub mod vector;
