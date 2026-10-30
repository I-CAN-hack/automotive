use std::time::Duration;

use crate::{can::Identifier, Result};

/// ECU byte order from its description file; CCP does not negotiate it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ByteOrder {
    #[default]
    LittleEndian,
    BigEndian,
}

impl ByteOrder {
    pub(super) fn encode_u32(self, value: u32) -> [u8; 4] {
        match self {
            Self::LittleEndian => value.to_le_bytes(),
            Self::BigEndian => value.to_be_bytes(),
        }
    }

    pub(super) fn u32(self, data: &[u8]) -> u32 {
        let bytes = [data[0], data[1], data[2], data[3]];
        match self {
            Self::LittleEndian => u32::from_le_bytes(bytes),
            Self::BigEndian => u32::from_be_bytes(bytes),
        }
    }
}

/// CCP CAN identifiers, adapter bus, command deadline and ECU address byte order.
/// Supports 11-bit and 29-bit IDs. Station addresses are always little endian.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CCPConfig {
    /// CRO transmit identifier.
    pub tx_id: Identifier,
    /// DTO receive identifier.
    pub rx_id: Identifier,
    #[cfg_attr(feature = "serde", serde(default))]
    pub bus: u8,
    #[cfg_attr(feature = "serde", serde(default = "default_timeout"))]
    pub timeout: Duration,
    #[cfg_attr(feature = "serde", serde(default))]
    pub byte_order: ByteOrder,
}

fn default_timeout() -> Duration {
    Duration::from_secs(1)
}

impl CCPConfig {
    /// Use bus zero, a one-second deadline and little-endian ECU addresses.
    pub fn new(tx_id: Identifier, rx_id: Identifier) -> Self {
        Self {
            tx_id,
            rx_id,
            bus: 0,
            timeout: default_timeout(),
            byte_order: ByteOrder::LittleEndian,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.tx_id == self.rx_id {
            return Err(crate::Error::InvalidAdapterConfig(
                "CCP transmit and receive identifiers must differ".into(),
            ));
        }
        if self.timeout.is_zero()
            || std::time::Instant::now()
                .checked_add(self.timeout)
                .is_none()
        {
            return Err(super::Error::InvalidArgument(
                "timeout must be positive and representable".into(),
            )
            .into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

/// EXCHANGE_ID sets MTA0; use UPLOAD to read `length` identification bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identification {
    pub length: u8,
    pub data_type: u8,
    pub resource_availability: u8,
    pub resource_protection: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed {
    pub protected: bool,
    pub data: [u8; 4],
}

/// Post-incremented MTA0 returned by DNLOAD and DNLOAD_6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryAddress {
    pub address: u32,
    pub address_extension: u8,
}
