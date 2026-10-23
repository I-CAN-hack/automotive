//! SAE J2534 04.04 PassThru adapter support.
//!
//! Provides raw CAN access and a native ISO-TP transport:
//!
//! * [`J2534Adapter`] — raw CAN channel implementing [`CanAdapter`](crate::can::CanAdapter).
//!   Paired with the software ISO-TP layer ([`IsoTP`](crate::transports::isotp::IsoTP)) for
//!   UDS communication.
//! * [`J2534NativeIsoTP`] — ISO 15765 channel implementing [`Transport`](crate::transports::Transport).
//!   The adapter firmware handles all ISO-TP framing in hardware; the host exchanges complete
//!   UDS PDUs.
//!
//! Both backends auto-discover the PassThru DLL from the Windows registry, or accept an
//! explicit DLL path.
//!
//! # Feature gate
//!
//! This module is only available on Windows with the `j2534` feature enabled.

mod can_adapter;
mod common;
mod constants;
mod dll;
mod error;
mod isotp;

pub use can_adapter::J2534Adapter;
pub use common::J2534Device;
pub use error::Error;
pub use isotp::J2534NativeIsoTP;
