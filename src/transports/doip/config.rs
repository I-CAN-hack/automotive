use std::{net::SocketAddr, time::Duration};

use super::Error;

/// TCP routing and framing settings. Only the ECU logical address is required.
///
/// The default protocol version is 0x02 for compatibility with deployed ECUs.
/// Versions 0x03 (2019) and 0x04 (2025) can be selected explicitly. This transport
/// implements unsecured TCP; TLS and manufacturer-specific authentication are
/// not implemented.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DoIPConfig {
    pub target_address: u16,
    /// Tester logical address; defaults to 0x0e00.
    pub source_address: u16,
    pub protocol_version: u8,
    /// Default routing activation (0x00).
    pub activation_type: u8,
    pub activation_oem: Option<[u8; 4]>,
    /// Optional local IP/port. Port zero chooses an ephemeral port. For IPv6
    /// link-local addresses, include the interface index in the scope ID.
    /// Omit this to let the OS select the adapter using its routing table.
    pub local_address: Option<SocketAddr>,
    pub connect_timeout: Duration,
    /// Routing activation deadline; defaults to two seconds (ISO 13400-2 Table 12).
    pub activation_timeout: Duration,
    /// Diagnostic ACK and socket-write deadline; defaults to two seconds.
    pub ack_timeout: Duration,
    /// Maximum DoIP payload, including logical addresses; defaults to 4 MiB.
    /// Bounds peer-controlled allocation before the payload is read.
    pub max_payload_size: usize,
}

impl DoIPConfig {
    pub fn new(target_address: u16) -> Self {
        Self {
            target_address,
            source_address: 0x0e00,
            protocol_version: 0x02,
            activation_type: 0,
            activation_oem: None,
            local_address: None,
            connect_timeout: Duration::from_secs(2),
            activation_timeout: Duration::from_secs(2),
            ack_timeout: Duration::from_secs(2),
            max_payload_size: 4 * 1024 * 1024,
        }
    }

    pub fn builder(target_address: u16) -> DoIPConfigBuilder {
        DoIPConfigBuilder(Self::new(target_address))
    }

    pub(super) fn validate(&self) -> crate::Result<()> {
        if !(2..=4).contains(&self.protocol_version) {
            return Err(Error::InvalidConfig("protocol_version must be 2, 3, or 4".into()).into());
        }
        if self.connect_timeout.is_zero()
            || self.activation_timeout.is_zero()
            || self.ack_timeout.is_zero()
            || !(13..=u32::MAX as usize).contains(&self.max_payload_size)
        {
            return Err(Error::InvalidConfig(
                "timeouts must be positive and max_payload_size must be 13..=u32::MAX".into(),
            )
            .into());
        }
        Ok(())
    }
}

/// Optional overrides for [`DoIPConfig::new`]. Public config fields can also be set directly.
#[derive(Debug, Clone)]
#[must_use]
pub struct DoIPConfigBuilder(DoIPConfig);

impl DoIPConfigBuilder {
    pub fn source_address(mut self, value: u16) -> Self {
        self.0.source_address = value;
        self
    }
    pub fn protocol_version(mut self, value: u8) -> Self {
        self.0.protocol_version = value;
        self
    }
    pub fn activation_type(mut self, value: u8) -> Self {
        self.0.activation_type = value;
        self
    }
    pub fn activation_oem(mut self, value: [u8; 4]) -> Self {
        self.0.activation_oem = Some(value);
        self
    }
    pub fn local_address(mut self, value: SocketAddr) -> Self {
        self.0.local_address = Some(value);
        self
    }
    pub fn connect_timeout(mut self, value: Duration) -> Self {
        self.0.connect_timeout = value;
        self
    }
    pub fn activation_timeout(mut self, value: Duration) -> Self {
        self.0.activation_timeout = value;
        self
    }
    pub fn ack_timeout(mut self, value: Duration) -> Self {
        self.0.ack_timeout = value;
        self
    }
    pub fn max_payload_size(mut self, value: usize) -> Self {
        self.0.max_payload_size = value;
        self
    }
    pub fn build(self) -> crate::Result<DoIPConfig> {
        self.0.validate()?;
        Ok(self.0)
    }
}
