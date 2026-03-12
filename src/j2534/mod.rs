//! SAE J2534 04.04 PassThru adapter support.
//!
//! Provides two adapters:
//!
//! * [`J2534CanAdapter`] — raw CAN channel implementing [`CanAdapter`](crate::can::CanAdapter).
//!   Paired with the software ISO-TP layer ([`IsoTPAdapter`](crate::isotp::IsoTPAdapter)) for
//!   UDS communication.
//! * [`J2534NativeIsoTpTransport`] — ISO 15765 channel implementing [`IsoTpTransport`](crate::IsoTpTransport).
//!   The adapter firmware handles all ISO-TP framing in hardware; the host exchanges complete
//!   UDS PDUs.
//!
//! Both adapters auto-discover the PassThru DLL from the Windows registry, or accept an
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
mod isotp_adapter;

pub use can_adapter::J2534CanAdapter;
pub use error::Error;
pub use isotp_adapter::J2534NativeIsoTpTransport;
