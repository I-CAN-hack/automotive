use std::time::Duration;

/// ECU byte order from its description file; CCP does not negotiate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ByteOrder {
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

/// Command deadline and ECU address byte order. Station addresses are always LE.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct CCPConfig {
    pub timeout: Duration,
    pub byte_order: ByteOrder,
}

impl Default for CCPConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(1),
            byte_order: ByteOrder::LittleEndian,
        }
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
