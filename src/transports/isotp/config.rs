use std::time::Duration;

use super::{duration_to_stmin_byte, Error, DEFAULT_PADDING_BYTE, DEFAULT_TIMEOUT_MS};
use crate::can::{Identifier, DLC_TO_LEN};
use crate::Result;

/// Configuration passed to the [`super::IsoTPAdapter`].
///
/// Use [`Self::builder`] for checked construction with automatic receive IDs.
/// Only the transmit identifier is required. A standard transmit ID defaults
/// to a receive ID eight higher, bus zero, Classical CAN, and padding enabled:
///
/// ```
/// use automotive::can::Identifier;
/// use automotive::transports::isotp::IsoTPConfig;
///
/// # fn main() -> automotive::Result<()> {
/// let config = IsoTPConfig::builder(Identifier::standard(0x7a1)?).build()?;
/// assert_eq!(config.rx_id, Identifier::standard(0x7a9)?);
/// # Ok(())
/// # }
/// ```
///
/// Supply an explicit receive ID for an application-specific mapping, or an
/// offset for a standard ID:
///
/// ```
/// use automotive::can::Identifier;
/// use automotive::transports::isotp::IsoTPConfig;
/// use std::time::Duration;
///
/// # fn main() -> automotive::Result<()> {
/// let explicit = IsoTPConfig::builder(Identifier::standard(0x7ff)?)
///     .rx_id(Identifier::standard(0x650)?)
///     .bus(1)
///     .timeout(Duration::from_secs(1))
///     .without_padding()
///     .build()?;
///
/// let offset = IsoTPConfig::builder(Identifier::standard(0x700)?)
///     .rx_id_offset(0x6a)
///     .build()?;
/// assert_eq!(offset.rx_id, Identifier::standard(0x76a)?);
/// # Ok(())
/// # }
/// ```
///
/// CAN FD defaults to a 64-byte transmit data link length; choose another valid
/// length with `max_dlen()`. Physical normal fixed addressing swaps the address
/// bytes to derive the response ID. Receive flow-control settings advertise how
/// quickly the peer may send consecutive frames:
///
/// ```
/// use automotive::can::Identifier;
/// use automotive::transports::isotp::IsoTPConfig;
/// use std::time::Duration;
///
/// # fn main() -> automotive::Result<()> {
/// let config = IsoTPConfig::builder(Identifier::extended(0x18da10f1)?)
///     .fd(true)
///     .max_dlen(32)
///     .block_size(8)
///     .receive_separation_time_min(Duration::from_micros(300))
///     .build()?;
/// assert_eq!(config.rx_id, Identifier::extended(0x18daf110)?);
/// # Ok(())
/// # }
/// ```
///
/// For extended addressing, configure the first data byte in each direction.
/// Omitting `rx_ext_address()` uses the transmit address for reception too:
///
/// ```
/// use automotive::can::Identifier;
/// use automotive::transports::isotp::IsoTPConfig;
///
/// # fn main() -> automotive::Result<()> {
/// let config = IsoTPConfig::builder(Identifier::standard(0x700)?)
///     .rx_id(Identifier::standard(0x708)?)
///     .ext_address(0x10)
///     .rx_ext_address(0xf1)
///     .build()?;
/// # Ok(())
/// # }
/// ```
///
/// `build()` returns an error for an overflowing derived receive ID, an invalid
/// transmit length, a zero timeout, or receive STmin above 127 ms.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IsoTPConfig {
    pub bus: u8,
    pub tx_id: Identifier,
    pub rx_id: Identifier,
    /// Padding byte. `None` disables optional padding; FD still pads to a valid DLC.
    pub padding: Option<u8>,
    /// Maximum time to wait for an individual received frame.
    pub timeout: Duration,
    /// Override the peer's STmin for transmitted consecutive frames.
    /// `None` follows the peer's flow-control settings.
    pub separation_time_min: Option<Duration>,
    pub fd: bool,
    /// First data byte for extended/mixed addressing, used for transmission.
    /// Also used for reception unless `rx_ext_address` is specified.
    pub ext_address: Option<u8>,
    /// Override the first address byte expected on received frames.
    #[cfg_attr(feature = "serde", serde(default))]
    pub rx_ext_address: Option<u8>,
    /// Transmit data link length (TX_DL), including any address byte.
    /// Defaults to eight bytes for Classical CAN or 64 for CAN FD.
    pub max_dlen: Option<usize>,
    /// Number of consecutive frames allowed per receive block; zero is unlimited.
    #[cfg_attr(feature = "serde", serde(default))]
    pub block_size: u8,
    /// STmin advertised to the peer in outgoing flow-control frames.
    #[cfg_attr(feature = "serde", serde(default))]
    pub receive_separation_time_min: Duration,
}

impl IsoTPConfig {
    /// Start a configuration with bus zero, Classical CAN, padding enabled,
    /// a 100 ms frame timeout, and unlimited receive blocks with zero STmin.
    ///
    /// Unless overridden, the receive ID is derived by adding eight to a
    /// standard ID or swapping the low two bytes of an extended ID. These are
    /// convenience conventions, not a general ISO-TP requirement. The byte swap
    /// matches physical normal fixed addressing (ISO 15765-2:2016, §10.3.3).
    /// Use [`IsoTPConfigBuilder::rx_id`] for other mappings.
    pub fn builder(tx_id: Identifier) -> IsoTPConfigBuilder {
        IsoTPConfigBuilder {
            config: Self {
                bus: 0,
                tx_id,
                rx_id: tx_id,
                padding: Some(DEFAULT_PADDING_BYTE),
                timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
                separation_time_min: None,
                fd: false,
                ext_address: None,
                rx_ext_address: None,
                max_dlen: None,
                block_size: 0,
                receive_separation_time_min: Duration::ZERO,
            },
            receive_id: ReceiveId::Automatic,
        }
    }

    /// Validate public fields again before using them for transport arithmetic.
    pub(super) fn validate(&self) -> Result<()> {
        let length = self.max_dlen.unwrap_or(if self.fd { 64 } else { 8 });
        if length < 8 || !DLC_TO_LEN.contains(&length) || (!self.fd && length != 8) {
            return Err(Error::InvalidConfig(
                "TX_DL must be 8 for CAN or 8/12/16/20/24/32/48/64 for CAN FD",
            )
            .into());
        }
        if self.timeout.is_zero() {
            return Err(Error::InvalidConfig("frame timeout must be greater than zero").into());
        }
        if self.rx_ext_address.is_some() && self.ext_address.is_none() {
            return Err(Error::InvalidConfig(
                "a receive address byte requires a transmit address byte",
            )
            .into());
        }
        let stmin = self.receive_separation_time_min;
        if stmin > Duration::from_millis(127) {
            return Err(Error::InvalidConfig("receive STmin must not exceed 127 ms").into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum ReceiveId {
    Automatic,
    Offset(u32),
    Explicit(Identifier),
}

/// Builder for an ISO-TP configuration.
///
/// `build()` checks receive ID arithmetic, TX_DL (ISO 15765-2:2016, §9.5.1),
/// and receive STmin (§9.6.5.4). Setters consume and return the builder.
/// Start with [`IsoTPConfig::builder`]; see [`IsoTPConfig`] for examples.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct IsoTPConfigBuilder {
    config: IsoTPConfig,
    receive_id: ReceiveId,
}

impl IsoTPConfigBuilder {
    pub fn bus(mut self, bus: u8) -> Self {
        self.config.bus = bus;
        self
    }

    /// Override automatic receive ID calculation. The last ID/offset setter wins.
    pub fn rx_id(mut self, rx_id: Identifier) -> Self {
        self.receive_id = ReceiveId::Explicit(rx_id);
        self
    }

    /// Add an offset to a standard transmit ID. Extended IDs require `rx_id()`.
    pub fn rx_id_offset(mut self, offset: u32) -> Self {
        self.receive_id = ReceiveId::Offset(offset);
        self
    }

    pub fn padding(mut self, byte: u8) -> Self {
        self.config.padding = Some(byte);
        self
    }

    pub fn without_padding(mut self) -> Self {
        self.config.padding = None;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Override transmit STmin. Omitting this setter follows the peer's STmin.
    pub fn separation_time_min(mut self, duration: Duration) -> Self {
        self.config.separation_time_min = Some(duration);
        self
    }

    pub fn fd(mut self, fd: bool) -> Self {
        self.config.fd = fd;
        self
    }

    /// Set a shared extended/mixed address byte for transmission and reception.
    pub fn ext_address(mut self, address: u8) -> Self {
        self.config.ext_address = Some(address);
        self
    }

    /// Expect a different address byte on reception (extended addressing).
    /// Requires `ext_address()` to configure the transmit address as well.
    pub fn rx_ext_address(mut self, address: u8) -> Self {
        self.config.rx_ext_address = Some(address);
        self
    }

    /// Set TX_DL in bytes: eight for Classical CAN; 8, 12, 16, 20, 24, 32,
    /// 48, or 64 for CAN FD. The default follows `fd()` at build time.
    pub fn max_dlen(mut self, length: usize) -> Self {
        self.config.max_dlen = Some(length);
        self
    }

    /// Advertise a receive block size. Zero requests all remaining frames.
    pub fn block_size(mut self, size: u8) -> Self {
        self.config.block_size = size;
        self
    }

    /// Advertise receive STmin, rounded up to the next representable wire value.
    /// Values above 127 ms are rejected rather than shortened.
    pub fn receive_separation_time_min(mut self, duration: Duration) -> Self {
        self.config.receive_separation_time_min = duration;
        self
    }

    /// Resolve automatic settings and reject invalid configuration values.
    pub fn build(mut self) -> Result<IsoTPConfig> {
        self.config.rx_id = match (self.config.tx_id, self.receive_id) {
            (_, ReceiveId::Explicit(id)) => id,
            (Identifier::Extended(id), ReceiveId::Automatic) => {
                let [a, b, c, d] = id.as_raw().to_be_bytes();
                Identifier::extended(u32::from_be_bytes([a, b, d, c]))?
            }
            (Identifier::Extended(_), ReceiveId::Offset(_)) => {
                return Err(
                    Error::InvalidConfig("receive ID offsets require a standard ID").into(),
                );
            }
            (Identifier::Standard(id), receive_id) => {
                let offset = match receive_id {
                    ReceiveId::Automatic => 8,
                    ReceiveId::Offset(offset) => offset,
                    ReceiveId::Explicit(_) => unreachable!(),
                };
                let raw = id
                    .as_raw()
                    .checked_add(offset)
                    .filter(|&id| id <= 0x7ff)
                    .ok_or(Error::InvalidConfig("receive identifier exceeds 11 bits"))?;
                Identifier::standard(raw)?
            }
        };
        self.config.validate()?;
        let stmin = self.config.receive_separation_time_min;
        let byte = duration_to_stmin_byte(stmin);
        self.config.receive_separation_time_min = if byte >= 0xf1 {
            Duration::from_micros(u64::from(byte - 0xf0) * 100)
        } else {
            Duration::from_millis(u64::from(byte))
        };
        Ok(self.config)
    }
}
