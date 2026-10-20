//! Frame rules follow ISO 11898-1:2015, 8.4.2–8.4.3 and 10.4.4.

use super::{Identifier, DLC_TO_LEN};

/// A Classical CAN data frame, containing zero to eight bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ClassicFrame {
    id: Identifier,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "deserialize_classic_data")
    )]
    data: Vec<u8>,
}

/// A CAN FD data frame. BRS and ESI exist only on this frame type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct FdFrame {
    id: Identifier,
    #[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_fd_data"))]
    data: Vec<u8>,
    brs: bool,
    /// A recessive ESI bit indicates an error-passive transmitter.
    esi: bool,
}

/// A Classical CAN remote request, with a DLC and no payload.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct RemoteFrame {
    id: Identifier,
    #[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_dlc"))]
    dlc: u8,
}

impl ClassicFrame {
    pub fn id(&self) -> Identifier {
        self.id
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl FdFrame {
    pub fn id(&self) -> Identifier {
        self.id
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn brs(&self) -> bool {
        self.brs
    }

    pub fn esi(&self) -> bool {
        self.esi
    }
}

impl RemoteFrame {
    pub fn id(&self) -> Identifier {
        self.id
    }

    /// The raw four-bit DLC. Codes 8 through 15 request eight bytes.
    pub fn dlc(&self) -> u8 {
        self.dlc
    }

    pub fn requested_len(&self) -> usize {
        self.dlc.min(8) as usize
    }
}

/// Error flags defined by ISO 11898-1:2015, 10.4.4.2.
///
/// Error frames contain an error flag and delimiter, with no identifier, DLC,
/// or data field. Adapters do not currently transmit these frames. These are
/// wire error flags, rather than operating-system-specific error reports.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ErrorFlag {
    Active,
    Passive,
}

/// The mutually exclusive CAN frame types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FrameContent {
    Classic(ClassicFrame),
    Fd(FdFrame),
    Remote(RemoteFrame),
    Error(ErrorFlag),
}

/// A validated CAN frame and adapter metadata.
///
/// Payloads remain `Vec<u8>` internally. Lengths are checked on construction
/// and deserialization; shared slices prevent resizing a valid frame.
///
/// ```
/// use automotive::can::{Frame, Identifier};
/// # fn example() -> automotive::Result<()> {
/// let id = Identifier::standard(0x123)?;
/// let classic = Frame::builder(id).bus(1).data([1, 2]).build()?;
/// let fd = Frame::builder(id).fd().brs(true).data([0; 12]).build()?;
/// let remote = Frame::remote_builder(id, 8).build()?;
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
    /// Compatibility constructor: selects FD for payloads longer than eight
    /// bytes. Use the builder to choose FD explicitly, including short frames.
    pub fn new(bus: u8, id: Identifier, data: &[u8]) -> crate::Result<Self> {
        let builder = Self::builder(id).bus(bus).data(data);
        if data.len() > 8 {
            builder.fd().build()
        } else {
            builder.build()
        }
    }

    /// Start a Classical data frame. The identifier is required; bus defaults
    /// to zero and the payload defaults to empty. Call `.fd()` to select FD.
    pub fn builder(id: Identifier) -> FrameBuilder<ClassicFrame> {
        FrameBuilder::new(ClassicFrame {
            id,
            data: Vec::new(),
        })
    }

    /// Start a remote request. Payload and FD options are unavailable.
    pub fn remote_builder(id: Identifier, dlc: u8) -> FrameBuilder<RemoteFrame> {
        FrameBuilder::new(RemoteFrame { id, dlc })
    }

    /// Represent an error frame. Adapter transmission is currently unsupported.
    pub fn error_builder(flag: ErrorFlag) -> FrameBuilder<ErrorFlag> {
        FrameBuilder::new(flag)
    }

    pub fn content(&self) -> &FrameContent {
        &self.content
    }

    /// Error frames have no arbitration identifier.
    pub fn id(&self) -> Option<Identifier> {
        match &self.content {
            FrameContent::Classic(frame) => Some(frame.id),
            FrameContent::Fd(frame) => Some(frame.id),
            FrameContent::Remote(frame) => Some(frame.id),
            FrameContent::Error(_) => None,
        }
    }

    /// Remote and error frames have no payload and return an empty slice.
    pub fn data(&self) -> &[u8] {
        match &self.content {
            FrameContent::Classic(frame) => &frame.data,
            FrameContent::Fd(frame) => &frame.data,
            FrameContent::Remote(_) | FrameContent::Error(_) => &[],
        }
    }

    /// The wire DLC, or `None` for error frames. Data frame DLCs are derived
    /// from the validated payload length; remote requests retain their own DLC.
    pub fn dlc(&self) -> Option<u8> {
        match &self.content {
            FrameContent::Classic(frame) => Some(frame.data.len() as u8),
            FrameContent::Fd(frame) => Some(
                DLC_TO_LEN
                    .iter()
                    .position(|&len| len == frame.data.len())
                    .unwrap() as u8,
            ),
            FrameContent::Remote(frame) => Some(frame.dlc),
            FrameContent::Error(_) => None,
        }
    }

    pub fn is_data(&self) -> bool {
        matches!(self.content, FrameContent::Classic(_) | FrameContent::Fd(_))
    }

    pub fn is_fd(&self) -> bool {
        matches!(self.content, FrameContent::Fd(_))
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.content, FrameContent::Remote(_))
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

/// Consuming builder whose methods depend on the selected frame type.
///
/// Invalid lengths return [`crate::Error::MalformedFrame`] from `build()`.
/// Data is copied as supplied, without implicit padding or truncation.
///
/// FD-only options cannot be set on a Classical frame:
/// ```compile_fail
/// # use automotive::can::{Frame, Identifier};
/// Frame::builder(Identifier::standard(0x123).unwrap()).brs(true);
/// ```
/// Remote requests cannot contain data or select FD:
/// ```compile_fail
/// # use automotive::can::{Frame, Identifier};
/// Frame::remote_builder(Identifier::standard(0x123).unwrap(), 8).data([1, 2]);
/// ```
/// ```compile_fail
/// # use automotive::can::{Frame, Identifier};
/// Frame::remote_builder(Identifier::standard(0x123).unwrap(), 8).fd();
/// ```
/// Error frames have neither identifiers nor payloads:
/// ```compile_fail
/// # use automotive::can::{Frame, ErrorFlag};
/// Frame::error_builder(ErrorFlag::Active).data([1]);
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct FrameBuilder<T = ClassicFrame> {
    bus: u8,
    loopback: bool,
    state: T,
}

impl<T> FrameBuilder<T> {
    fn new(state: T) -> Self {
        Self {
            bus: 0,
            loopback: false,
            state,
        }
    }

    pub fn bus(mut self, bus: u8) -> Self {
        self.bus = bus;
        self
    }

    pub fn loopback(mut self, loopback: bool) -> Self {
        self.loopback = loopback;
        self
    }

    fn finish(self, into_content: impl FnOnce(T) -> FrameContent) -> Frame {
        Frame {
            bus: self.bus,
            loopback: self.loopback,
            content: into_content(self.state),
        }
    }
}

impl FrameBuilder<ClassicFrame> {
    pub fn data(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.state.data = data.into();
        self
    }

    pub fn fd(self) -> FrameBuilder<FdFrame> {
        FrameBuilder {
            bus: self.bus,
            loopback: self.loopback,
            state: FdFrame {
                id: self.state.id,
                data: self.state.data,
                brs: false,
                esi: false,
            },
        }
    }

    pub fn build(self) -> crate::Result<Frame> {
        validate_classic_data(&self.state.data)?;
        Ok(self.finish(FrameContent::Classic))
    }
}

impl FrameBuilder<FdFrame> {
    pub fn data(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.state.data = data.into();
        self
    }

    pub fn brs(mut self, brs: bool) -> Self {
        self.state.brs = brs;
        self
    }

    pub fn esi(mut self, esi: bool) -> Self {
        self.state.esi = esi;
        self
    }

    pub fn build(self) -> crate::Result<Frame> {
        validate_fd_data(&self.state.data)?;
        Ok(self.finish(FrameContent::Fd))
    }
}

impl FrameBuilder<RemoteFrame> {
    pub fn build(self) -> crate::Result<Frame> {
        validate_dlc(self.state.dlc)?;
        Ok(self.finish(FrameContent::Remote))
    }
}

impl FrameBuilder<ErrorFlag> {
    pub fn build(self) -> crate::Result<Frame> {
        Ok(self.finish(FrameContent::Error))
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
) -> Result<Vec<u8>, D::Error> {
    use serde::Deserialize;
    let data = Vec::<u8>::deserialize(deserializer)?;
    validate_classic_data(&data).map_err(serde::de::Error::custom)?;
    Ok(data)
}

#[cfg(feature = "serde")]
fn deserialize_fd_data<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<u8>, D::Error> {
    use serde::Deserialize;
    let data = Vec::<u8>::deserialize(deserializer)?;
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
