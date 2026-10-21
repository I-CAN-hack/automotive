//! Types used in the UDS protocol.
use super::Error;
use std::time::Duration;

/// Outcome of a custom UDS request.
///
/// Negative responses remain [`super::Error::NegativeResponse`] errors. Response
/// Pending (NRC 0x78) is handled internally until a final response or timeout.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestOutcome<T> {
    /// Data from a received positive response, excluding the response headers.
    PositiveResponse(T),
    /// The initial response window elapsed without a response when suppression
    /// was requested. This does not confirm that the ECU executed the request.
    Suppressed,
}

/// Struct returned by DiagnosticSessionControl (0x10)
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SessionParameterRecord {
    /// Performance requirement for the server (i.e. the ECU) to start with th response message after the reception of a request message.
    pub p2_server_max: Duration,
    /// Performance requirement for the server (i.e. the ECU) to start with the response message after the transmission of a "ResponsePending" message.
    pub p2_star_server_max: Duration,
}

uds_code_enum! {
    /// DTC format identifiers defined in ISO 14229, Annex D.
    #[allow(non_camel_case_types)]
    pub enum DTCFormatIdentifier: u8 {
        SAE_J2012_DA_DTCFormat_00 = 0x00,
        ISO_14229_1_DTCFormat = 0x01,
        SAE_J1939_73_DTCFormat = 0x02,
        ISO_11992_4_DTCFormat = 0x03,
        SAE_J2012_DA_DTCFormat_04 = 0x04,
    }
}

impl DTCFormatIdentifier {
    /// Decode a named format, returning `None` for values without a named variant.
    /// Use [`Self::from_raw`] to preserve unknown formats instead.
    pub const fn from_repr(value: u8) -> Option<Self> {
        match Self::from_raw(value) {
            Self::NonStandard(_) => None,
            format => Some(format),
        }
    }
}

/// Struct returned by ReadDTCInformation (0x19)
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DTCReportNumberByStatusMask {
    pub dtc_status_availability_mask: u8,
    pub dtc_format_identifier: DTCFormatIdentifier,
    pub dtc_count: u16,
}

/// Download/upload compression method, occupying the high nibble of the
/// data format identifier. Zero means no compression; other values are
/// manufacturer-specific. Values must fit in `0..=0xf`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
pub struct CompressionMethod(u8);

impl CompressionMethod {
    /// No compression applied (wire value zero).
    pub const NONE: Self = Self(0);

    /// Construct a method, rejecting values larger than one nibble.
    pub const fn new(value: u8) -> Result<Self, Error> {
        if value <= 0xf {
            Ok(Self(value))
        } else {
            Err(Error::InvalidCompressionMethod(value))
        }
    }

    /// Return the unshifted four-bit method value.
    pub const fn as_raw(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for CompressionMethod {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CompressionMethod> for u8 {
    fn from(value: CompressionMethod) -> Self {
        value.as_raw()
    }
}

/// Download/upload encryption method, occupying the low nibble of the
/// data format identifier. Zero means no encryption; other values are
/// manufacturer-specific. Values must fit in `0..=0xf`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
pub struct EncryptionMethod(u8);

impl EncryptionMethod {
    /// No encryption applied (wire value zero).
    pub const NONE: Self = Self(0);

    /// Construct a method, rejecting values larger than one nibble.
    pub const fn new(value: u8) -> Result<Self, Error> {
        if value <= 0xf {
            Ok(Self(value))
        } else {
            Err(Error::InvalidEncryptionMethod(value))
        }
    }

    /// Return the unshifted four-bit method value.
    pub const fn as_raw(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for EncryptionMethod {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<EncryptionMethod> for u8 {
    fn from(value: EncryptionMethod) -> Self {
        value.as_raw()
    }
}
