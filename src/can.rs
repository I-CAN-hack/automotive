#[derive(Debug, Clone)]
pub enum Identifier {
    Standard(u32),
    Extended(u32),
}

#[derive(Debug, Clone)]
pub struct Frame {
    id: Identifier,
    data: Vec<u8>,
    // TODO: Add timestamp, can-fd, rtr
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
