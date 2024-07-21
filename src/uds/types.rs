//! Types used in the UDS protocol.
use strum_macros::FromRepr;

use std::time::Duration;

/// Struct returned by DiagnosticSessionControl (0x10)
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SessionParameterRecord {
    /// Performance requirement for the server (i.e. the ECU) to start with th response message after the reception of a request message.
    pub p2_server_max: Duration,
    /// Performance requirement for the server (i.e. the ECU) to start with the response message after the transmission of a "ResponsePending" message.
    pub p2_star_server_max: Duration,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, FromRepr)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum DTCFormatIdentifier {
    SAE_J2012_DA_DTCFormat_00 = 0x00,
    ISO_14229_1_DTCFormat = 0x01,
    SAE_J1939_73_DTCFormat = 0x02,
    ISO_11992_4_DTCFormat = 0x03,
    SAE_J2012_DA_DTCFormat_04 = 0x04,
}

/// Struct returned by ReadDTCInformation (0x19)
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DTCReportNumberByStatusMask {
    pub dtc_status_availability_mask: u8,
    pub dtc_format_identifier: DTCFormatIdentifier,
    pub dtc_count: u16,
}
