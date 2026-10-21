use std::fmt;

/// A CAN arbitration identifier with an explicit wire format.
///
/// Frames validate the range on construction, so identifiers can be written
/// directly as `Identifier::Standard(0x123)` or `Identifier::Extended(0x123)`.
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Identifier {
    Standard(#[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_standard"))] u32),
    Extended(#[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_extended"))] u32),
}

impl Identifier {
    pub const fn standard(id: u32) -> crate::Result<Self> {
        Self::Standard(id).validate()
    }

    pub const fn extended(id: u32) -> crate::Result<Self> {
        Self::Extended(id).validate()
    }

    pub const fn validate(self) -> crate::Result<Self> {
        match self {
            Self::Standard(id) if id <= 0x7ff => Ok(self),
            Self::Extended(id) if id <= 0x1fff_ffff => Ok(self),
            _ => Err(crate::Error::MalformedFrame),
        }
    }

    pub const fn is_standard(self) -> bool {
        matches!(self, Self::Standard(_))
    }

    pub const fn is_extended(self) -> bool {
        matches!(self, Self::Extended(_))
    }

    pub const fn as_raw(self) -> u32 {
        match self {
            Self::Standard(id) | Self::Extended(id) => id,
        }
    }
}

/// Selects standard format for values up to 0x7ff, extended format otherwise.
impl TryFrom<u32> for Identifier {
    type Error = crate::Error;

    fn try_from(id: u32) -> crate::Result<Self> {
        if id <= 0x7ff {
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

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standard(id) => write!(f, "0x{:03x}", id),
            Self::Extended(id) => write!(f, "0x{:08x}", id),
        }
    }
}

#[cfg(feature = "serde")]
fn deserialize_standard<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u32, D::Error> {
    use serde::Deserialize;
    let id = u32::deserialize(deserializer)?;
    Identifier::standard(id)
        .map(Identifier::as_raw)
        .map_err(serde::de::Error::custom)
}

#[cfg(feature = "serde")]
fn deserialize_extended<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u32, D::Error> {
    use serde::Deserialize;
    let id = u32::deserialize(deserializer)?;
    Identifier::extended(id)
        .map(Identifier::as_raw)
        .map_err(serde::de::Error::custom)
}
