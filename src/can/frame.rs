//! Frame rules follow ISO 11898-1:2015, 8.4.2–8.4.3 and 10.4.4.

use super::{Identifier, DLC_TO_LEN};
use arrayvec::ArrayVec;

/// A wire error flag, rather than an operating-system-specific error report.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ErrorFlag {
    Active,
    Passive,
}

/// The mutually exclusive CAN frame types and their fields.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub enum FrameContent {
    Classic {
        id: Identifier,
        #[cfg_attr(
            feature = "serde",
            serde(deserialize_with = "deserialize_classic_data")
        )]
        data: ArrayVec<u8, 64>,
    },
    Fd {
        id: Identifier,
        #[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_fd_data"))]
        data: ArrayVec<u8, 64>,
        brs: bool,
        /// A recessive ESI bit indicates an error-passive transmitter.
        esi: bool,
    },
    /// A Classical CAN remote request, with a raw four-bit DLC and no payload.
    /// Codes 8 through 15 request eight bytes.
    Remote {
        id: Identifier,
        #[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_dlc"))]
        dlc: u8,
    },
    /// Error frames have no identifier, DLC, or payload.
    Error(ErrorFlag),
}

/// A validated CAN frame and adapter metadata.
///
/// Payloads are stored inline and checked on construction and deserialization.
/// Shared slices prevent resizing a valid frame.
///
/// ```
/// use automotive::can::{Frame, Identifier};
/// # fn example() -> automotive::Result<()> {
/// let id = Identifier::standard(0x123)?;
/// let classic = Frame::builder(id).bus(1).data(&[1, 2]).build()?;
/// let fd = Frame::builder(id).fd(true).brs(true).data(&[0; 12]).build()?;
/// let remote = Frame::remote(0, id, 8)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Frame {
    /// Bus index for adapters supporting multiple CAN buses.
    pub bus: u8,
    /// Whether the adapter has confirmed transmission of this frame.
    pub loopback: bool,
    content: FrameContent,
}

impl Frame {
    /// Selects FD for payloads longer than eight bytes. Use the builder to
    /// choose FD explicitly, including short frames.
    pub fn new(bus: u8, id: Identifier, data: &[u8]) -> crate::Result<Self> {
        Self::builder(id)
            .bus(bus)
            .data(data)
            .fd(data.len() > 8)
            .build()
    }

    /// Start a data frame. Bus defaults to zero and payload to empty.
    pub fn builder<'a>(id: Identifier) -> FrameBuilder<'a> {
        FrameBuilder {
            bus: 0,
            loopback: false,
            id,
            data: &[],
            fd: false,
            brs: false,
            esi: false,
        }
    }

    /// Create a remote request with a raw DLC (0–15) and no payload.
    pub fn remote(bus: u8, id: Identifier, dlc: u8) -> crate::Result<Self> {
        validate_dlc(dlc)?;
        Ok(Self {
            bus,
            loopback: false,
            content: FrameContent::Remote { id, dlc },
        })
    }

    /// Represent an error frame. Adapter transmission is currently unsupported.
    pub fn error(bus: u8, flag: ErrorFlag) -> Self {
        Self {
            bus,
            loopback: false,
            content: FrameContent::Error(flag),
        }
    }

    pub fn content(&self) -> &FrameContent {
        &self.content
    }

    /// Error frames have no arbitration identifier.
    pub fn id(&self) -> Option<Identifier> {
        match &self.content {
            FrameContent::Classic { id, .. }
            | FrameContent::Fd { id, .. }
            | FrameContent::Remote { id, .. } => Some(*id),
            FrameContent::Error(_) => None,
        }
    }

    /// Remote and error frames have no payload and return an empty slice.
    pub fn data(&self) -> &[u8] {
        match &self.content {
            FrameContent::Classic { data, .. } | FrameContent::Fd { data, .. } => data,
            FrameContent::Remote { .. } | FrameContent::Error(_) => &[],
        }
    }

    /// The wire DLC, or `None` for error frames.
    pub fn dlc(&self) -> Option<u8> {
        match &self.content {
            FrameContent::Classic { data, .. } | FrameContent::Fd { data, .. } => DLC_TO_LEN
                .iter()
                .position(|&len| len == data.len())
                .map(|dlc| dlc as u8),
            FrameContent::Remote { dlc, .. } => Some(*dlc),
            FrameContent::Error(_) => None,
        }
    }

    pub fn is_data(&self) -> bool {
        matches!(
            self.content,
            FrameContent::Classic { .. } | FrameContent::Fd { .. }
        )
    }

    pub fn is_fd(&self) -> bool {
        matches!(self.content, FrameContent::Fd { .. })
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.content, FrameContent::Remote { .. })
    }

    pub fn is_error(&self) -> bool {
        matches!(self.content, FrameContent::Error(_))
    }
}

impl std::fmt::Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/// Builder for Classical and FD data frames.
///
/// `build()` rejects invalid lengths and BRS/ESI on Classical frames with
/// [`crate::Error::MalformedFrame`]. Identifiers are checked when constructed.
/// Payload data is borrowed until `build()` copies it into the frame's inline
/// storage. Data is never padded or truncated.
#[derive(Debug, Clone)]
#[must_use]
pub struct FrameBuilder<'a> {
    bus: u8,
    loopback: bool,
    id: Identifier,
    data: &'a [u8],
    fd: bool,
    brs: bool,
    esi: bool,
}

impl<'a> FrameBuilder<'a> {
    pub fn bus(mut self, bus: u8) -> Self {
        self.bus = bus;
        self
    }

    pub fn loopback(mut self, loopback: bool) -> Self {
        self.loopback = loopback;
        self
    }

    /// Borrow a payload until `build()`, which checks its length before copying.
    pub fn data(mut self, data: &'a [u8]) -> Self {
        self.data = data;
        self
    }

    pub fn fd(mut self, fd: bool) -> Self {
        self.fd = fd;
        self
    }

    pub fn brs(mut self, brs: bool) -> Self {
        self.brs = brs;
        self
    }

    pub fn esi(mut self, esi: bool) -> Self {
        self.esi = esi;
        self
    }

    pub fn build(self) -> crate::Result<Frame> {
        if self.fd {
            validate_fd_data(self.data)?;
        } else {
            validate_classic_data(self.data)?;
            if self.brs || self.esi {
                return Err(crate::Error::MalformedFrame);
            }
        }
        let mut data = ArrayVec::new();
        data.try_extend_from_slice(self.data)
            .map_err(|_| crate::Error::MalformedFrame)?;
        let content = if self.fd {
            FrameContent::Fd {
                id: self.id,
                data,
                brs: self.brs,
                esi: self.esi,
            }
        } else {
            FrameContent::Classic { id: self.id, data }
        };
        Ok(Frame {
            bus: self.bus,
            loopback: self.loopback,
            content,
        })
    }
}

fn validate_classic_data(data: &[u8]) -> crate::Result<()> {
    if data.len() <= 8 {
        Ok(())
    } else {
        Err(crate::Error::MalformedFrame)
    }
}

fn validate_fd_data(data: &[u8]) -> crate::Result<()> {
    if DLC_TO_LEN.contains(&data.len()) {
        Ok(())
    } else {
        Err(crate::Error::MalformedFrame)
    }
}

fn validate_dlc(dlc: u8) -> crate::Result<()> {
    if dlc <= 15 {
        Ok(())
    } else {
        Err(crate::Error::MalformedFrame)
    }
}

#[cfg(feature = "serde")]
fn deserialize_classic_data<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArrayVec<u8, 64>, D::Error> {
    use serde::Deserialize;
    let data = ArrayVec::<u8, 64>::deserialize(deserializer)?;
    validate_classic_data(&data).map_err(serde::de::Error::custom)?;
    Ok(data)
}

#[cfg(feature = "serde")]
fn deserialize_fd_data<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArrayVec<u8, 64>, D::Error> {
    use serde::Deserialize;
    let data = ArrayVec::<u8, 64>::deserialize(deserializer)?;
    validate_fd_data(&data).map_err(serde::de::Error::custom)?;
    Ok(data)
}

#[cfg(feature = "serde")]
fn deserialize_dlc<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    use serde::Deserialize;
    let dlc = u8::deserialize(deserializer)?;
    validate_dlc(dlc).map_err(serde::de::Error::custom)?;
    Ok(dlc)
}
