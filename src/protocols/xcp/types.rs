use std::time::Duration;

/// Byte order negotiated by CONNECT. Ethernet LEN and CTR are always little endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ByteOrder {
    LittleEndian,
    BigEndian,
}

impl ByteOrder {
    pub(crate) fn u16(self, data: &[u8]) -> u16 {
        let bytes = [data[0], data[1]];
        match self {
            Self::LittleEndian => u16::from_le_bytes(bytes),
            Self::BigEndian => u16::from_be_bytes(bytes),
        }
    }

    pub(crate) fn u32(self, data: &[u8]) -> u32 {
        let bytes = [data[0], data[1], data[2], data[3]];
        match self {
            Self::LittleEndian => u32::from_le_bytes(bytes),
            Self::BigEndian => u32::from_be_bytes(bytes),
        }
    }

    pub(crate) fn encode_u32(self, value: u32) -> [u8; 4] {
        match self {
            Self::LittleEndian => value.to_le_bytes(),
            Self::BigEndian => value.to_be_bytes(),
        }
    }
}

/// XCP command deadline, including transmission and the complete response.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct XCPConfig {
    pub timeout: Duration,
}

impl Default for XCPConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(1),
        }
    }
}

/// Capabilities returned by CONNECT (ASAM XCP 1.5, section 7.5.1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConnectInfo {
    pub resource: u8,
    pub comm_mode_basic: u8,
    pub byte_order: ByteOrder,
    /// Bytes represented by one ECU address: 1, 2, or 4.
    pub address_granularity: u8,
    pub max_cto: u8,
    pub max_dto: u16,
    /// Major version bytes; CONNECT does not report the minor versions.
    pub protocol_version: u8,
    pub transport_version: u8,
}

impl ConnectInfo {
    /// Maximum byte count in one standard UPLOAD or SHORT_UPLOAD response.
    pub fn max_upload_size(&self) -> usize {
        let ag = usize::from(self.address_granularity);
        (usize::from(self.max_cto) / ag - 1) * ag
    }

    pub fn slave_block_mode(&self) -> bool {
        self.comm_mode_basic & 0x40 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub session_status: u8,
    pub resource_protection_status: u8,
    pub session_configuration_id: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommunicationModeInfo {
    pub comm_mode_optional: u8,
    pub max_bs: u8,
    pub min_st: u8,
    pub queue_size: u8,
    pub driver_version: u8,
}

/// GET_ID result. Mode zero sets MTA; use UPLOAD to obtain `length` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identification {
    pub mode: u8,
    pub length: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checksum {
    pub checksum_type: u8,
    pub checksum: u32,
}

/// GET_SEED response segment. Length counts this and all remaining seed bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seed {
    pub length: u8,
    pub data: Vec<u8>,
}
