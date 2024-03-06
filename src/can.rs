//! Generic CAN types and traits

/// Identifier for a CAN frame
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Hash)]
pub enum Identifier {
    Standard(u32),
    Extended(u32),
}

/// A CAN frame
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// The bus index for adapters supporting multiple CAN busses
    pub bus: u8,
    /// Arbitration ID
    pub id: Identifier,
    /// Frame Data
    pub data: Vec<u8>,
    /// Wheter the frame was sent out by the adapter
    pub returned: bool,
    // TODO: Add timestamp, can-fd, rtr, dlc
}
impl Unpin for Frame {}

impl Frame {
    pub fn new(bus: u8, id: Identifier, data: &[u8]) -> Frame {
        Frame {
            bus: bus,
            id: id,
            data: data.to_vec(),
            returned: false,
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

impl Into<u32> for Identifier {
    fn into(self) -> u32 {
        match self {
            Identifier::Standard(id) => id,
            Identifier::Extended(id) => id,
        }
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
        assert_eq!(
            true,
            Identifier::Standard(0x123) < Identifier::Standard(0x124)
        );
        assert_eq!(
            true,
            Identifier::Standard(0x7ff) > Identifier::Standard(0x100)
        );

        // Extended IDs always have lower priority than standard IDs
        assert_eq!(
            true,
            Identifier::Extended(0x1) > Identifier::Standard(0x100)
        );
    }
}
