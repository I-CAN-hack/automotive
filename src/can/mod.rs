//! Generic CAN types and traits

pub mod adapter;
pub mod async_can;

use std::collections::VecDeque;
use std::fmt;

pub use adapter::get_adapter;
pub use async_can::AsyncCanAdapter;
pub use embedded_can::{ExtendedId, Id, StandardId};

pub static DLC_TO_LEN: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

/// A CAN frame
#[derive(Clone, PartialEq)]
pub struct Frame {
    /// The bus index for adapters supporting multiple CAN busses
    pub bus: u8,
    /// Arbitration ID
    pub id: Id,
    /// Frame Data
    pub data: Vec<u8>,
    /// Wheter the frame was sent out by the adapter
    pub loopback: bool,
    /// CAN-FD Frame
    pub fd: bool,
    // TODO: Add timestamp, rtr, dlc
}
impl Unpin for Frame {}

impl Frame {
    pub fn new(bus: u8, id: Id, data: &[u8]) -> Result<Frame, crate::error::Error> {
        // Check if the data length is valid
        if !DLC_TO_LEN.contains(&data.len()) {
            return Err(crate::error::Error::MalformedFrame);
        }

        Ok(Frame {
            bus,
            id,
            data: data.to_vec(),
            loopback: false,
            fd: data.len() > 8,
        })
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("bus", &self.bus)
            .field("id", &self.id)
            .field("data", &hex::encode(&self.data))
            .field("loopback", &self.loopback)
            .field("fd", &self.fd)
            .finish()
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Trait for a Blocking CAN Adapter
pub trait CanAdapter {
    fn send(&mut self, frames: &mut VecDeque<crate::can::Frame>) -> crate::Result<()>;
    fn recv(&mut self) -> crate::Result<Vec<Frame>>;
}
