//! Types used in the UDS protocol.

use std::time::Duration;

/// Struct returned by DiagnosticSessionControl (0x10)
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct SessionParameterRecord {
    /// Performance requirement for the server (i.e. the ECU) to start with th response message after the reception of a request message.
    pub p2_server_max: Duration,
    /// Performance requirement for the server (i.e. the ECU) to start with the response message after the transmission of a "ResponsePending" message.
    pub p2_star_server_max: Duration,
}
