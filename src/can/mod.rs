//! Generic CAN types and traits

pub mod adapter;
pub mod async_can;

use std::fmt;

pub use adapter::get_adapter;
pub use async_can::AsyncCanAdapter;

pub static DLC_TO_LEN: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

/// Identifier for a CAN frame
#[derive(Copy, Clone, PartialOrd, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Identifier {
    Standard(u32),
    Extended(u32),
}

impl Identifier {
    pub fn is_standard(&self) -> bool {
        match self {
            Identifier::Standard(_) => true,
            Identifier::Extended(_) => false,
        }
    }
    pub fn is_extended(&self) -> bool {
        !self.is_standard()
    }
}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Identifier::Extended(id) => write!(f, "0x{:08x}", id),
            Identifier::Standard(id) => write!(f, "0x{:03x}", id),
        }
    }
}

impl From<u32> for Identifier {
    fn from(id: u32) -> Identifier {
        if id <= 0x7ff {
            Identifier::Standard(id)
        } else {
            Identifier::Extended(id)
        }
    }
}

impl From<Identifier> for u32 {
    fn from(val: Identifier) -> u32 {
        match val {
            Identifier::Standard(id) => id,
            Identifier::Extended(id) => id,
        }
    }
}

/// A CAN frame
#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Frame {
    /// The bus index for adapters supporting multiple CAN busses
    pub bus: u8,
    /// Arbitration ID
    pub id: Identifier,
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
    pub fn new(bus: u8, id: Identifier, data: &[u8]) -> Result<Frame, crate::error::Error> {
        // Check if the data length is valid
        if !DLC_TO_LEN.contains(&data.len()) {
            return Err(crate::error::Error::MalformedFrame);
        }

        // Check if the ID makes sense
        match id {
            Identifier::Standard(id) if id > 0x7ff => return Err(crate::error::Error::MalformedFrame),
            Identifier::Extended(id) if id > 0x1fffffff => return Err(crate::error::Error::MalformedFrame),
            _ => {}
        };

        Ok(Frame {
            bus,
            id,
            data: data.to_vec(),
            loopback: false,
            fd: data.len() > 8,
        })
    }
}

impl fmt::Debug for Frame {
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

/// Trait for a Blocking CAN Adapter
pub trait CanAdapter {
    fn send(&mut self, frames: &[Frame]) -> Result<(), crate::error::Error>;
    fn recv(&mut self) -> Result<Vec<Frame>, crate::error::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_compare() {
        assert_eq!(true, Identifier::Standard(0x123) < Identifier::Standard(0x124));
        assert_eq!(true, Identifier::Standard(0x7ff) > Identifier::Standard(0x100));

        // Extended IDs always have lower priority than standard IDs
        assert_eq!(true, Identifier::Extended(0x1) > Identifier::Standard(0x100));
    }
}
