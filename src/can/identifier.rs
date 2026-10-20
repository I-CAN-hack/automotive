use std::fmt;

/// An 11-bit CAN identifier. Construction and deserialization check the range.
#[derive(Debug, Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u32", into = "u32"))]
pub struct StandardId(u16);

impl StandardId {
    pub const MAX: u32 = 0x7ff;

    pub const fn new(id: u32) -> crate::Result<Self> {
        if id <= Self::MAX {
            Ok(Self(id as u16))
        } else {
            Err(crate::Error::MalformedFrame)
        }
    }

    pub const fn as_raw(self) -> u32 {
        self.0 as u32
    }
}

/// A 29-bit CAN identifier, including values that also fit in 11 bits.
#[derive(Debug, Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u32", into = "u32"))]
pub struct ExtendedId(u32);

impl ExtendedId {
    pub const MAX: u32 = 0x1fff_ffff;

    pub const fn new(id: u32) -> crate::Result<Self> {
        if id <= Self::MAX {
            Ok(Self(id))
        } else {
            Err(crate::Error::MalformedFrame)
        }
    }

    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

macro_rules! id_conversions {
    ($id:ty) => {
        impl TryFrom<u32> for $id {
            type Error = crate::Error;

            fn try_from(id: u32) -> crate::Result<Self> {
                Self::new(id)
            }
        }

        impl From<$id> for u32 {
            fn from(id: $id) -> u32 {
                id.as_raw()
            }
        }
    };
}

id_conversions!(StandardId);
id_conversions!(ExtendedId);

/// A validated CAN arbitration identifier with an explicit wire format.
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Identifier {
    Standard(StandardId),
    Extended(ExtendedId),
}

impl Identifier {
    pub fn standard(id: u32) -> crate::Result<Self> {
        StandardId::new(id).map(Self::Standard)
    }

    pub fn extended(id: u32) -> crate::Result<Self> {
        ExtendedId::new(id).map(Self::Extended)
    }

    pub const fn is_standard(self) -> bool {
        matches!(self, Self::Standard(_))
    }

    pub const fn is_extended(self) -> bool {
        matches!(self, Self::Extended(_))
    }

    pub const fn as_raw(self) -> u32 {
        match self {
            Self::Standard(id) => id.as_raw(),
            Self::Extended(id) => id.as_raw(),
        }
    }
}

/// Selects standard format for values up to 0x7ff, extended format otherwise.
impl TryFrom<u32> for Identifier {
    type Error = crate::Error;

    fn try_from(id: u32) -> crate::Result<Self> {
        if id <= StandardId::MAX {
            Self::standard(id)
        } else {
            Self::extended(id)
        }
    }
}

impl From<Identifier> for u32 {
    fn from(id: Identifier) -> Self {
        id.as_raw()
    }
}

impl From<StandardId> for Identifier {
    fn from(id: StandardId) -> Self {
        Self::Standard(id)
    }
}

impl From<ExtendedId> for Identifier {
    fn from(id: ExtendedId) -> Self {
        Self::Extended(id)
    }
}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standard(id) => write!(f, "0x{:03x}", id.as_raw()),
            Self::Extended(id) => write!(f, "0x{:08x}", id.as_raw()),
        }
    }
}
