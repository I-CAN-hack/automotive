use std::fmt;

/// An 11-bit value constructed through [`Identifier::standard`].
///
/// ```compile_fail
/// use automotive::can::StandardId;
/// let id = StandardId(0x800);
/// ```
#[derive(Debug, Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct StandardId(
    #[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_standard"))] u32,
);

impl StandardId {
    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

/// A 29-bit value constructed through [`Identifier::extended`].
///
/// ```compile_fail
/// use automotive::can::ExtendedId;
/// let id = ExtendedId(0x2000_0000);
/// ```
#[derive(Debug, Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ExtendedId(
    #[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_extended"))] u32,
);

impl ExtendedId {
    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

/// A CAN arbitration identifier, checked on construction and deserialization.
///
/// Use [`Self::standard`] or [`Self::extended`] to choose the wire format.
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Identifier {
    Standard(StandardId),
    Extended(ExtendedId),
}

impl Identifier {
    /// Construct a standard identifier, rejecting values above 0x7ff.
    pub const fn standard(id: u32) -> crate::Result<Self> {
        if id <= 0x7ff {
            Ok(Self::Standard(StandardId(id)))
        } else {
            Err(crate::Error::MalformedFrame)
        }
    }

    /// Construct an extended identifier, rejecting values above 0x1fff_ffff.
    /// Values that also fit in 11 bits retain the extended wire format.
    pub const fn extended(id: u32) -> crate::Result<Self> {
        if id <= 0x1fff_ffff {
            Ok(Self::Extended(ExtendedId(id)))
        } else {
            Err(crate::Error::MalformedFrame)
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
            Self::Standard(id) => id.as_raw(),
            Self::Extended(id) => id.as_raw(),
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
            Self::Standard(id) => write!(f, "0x{:03x}", id.as_raw()),
            Self::Extended(id) => write!(f, "0x{:08x}", id.as_raw()),
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
