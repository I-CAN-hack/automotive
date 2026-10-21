//! Scoped access to services with positive-response suppression enabled.

use super::{
    RequestOutcome, RoutineControlType, ServiceIdentifier, SessionParameterRecord, UDSClient,
};
use crate::{Result, TransportLayer};

/// A borrowed view of a [`UDSClient`] that suppresses positive responses.
///
/// Create it with [`UDSClient::suppress_positive_responses`]. Suppression is
/// local to this view; the underlying client and its session timings are shared.
/// The view exposes helpers whose return types can represent an absent response.
/// Use [`Self::request`] for other services with a sub-function parameter.
pub struct SuppressedUDSClient<'client, 'adapter, T: TransportLayer> {
    pub(super) client: &'client UDSClient<'adapter, T>,
}

impl<T: TransportLayer> SuppressedUDSClient<'_, '_, T> {
    /// Make a custom request with bit 7 set in its sub-function byte.
    /// Requests without a sub-function still require a response.
    pub async fn request(
        &self,
        sid: u8,
        sub_function: Option<u8>,
        data: Option<&[u8]>,
    ) -> Result<RequestOutcome<Vec<u8>>> {
        self.client
            .request(sid, sub_function.map(|value| value | 0x80), data)
            .await
    }

    /// Diagnostic Session Control with positive-response suppression.
    /// Silence returns `None` and preserves current session timings.
    pub async fn diagnostic_session_control(
        &self,
        session_type: u8,
    ) -> Result<Option<SessionParameterRecord>> {
        self.client
            .diagnostic_session_control(session_type | 0x80)
            .await
    }

    /// ECU Reset with positive-response suppression. Silence returns `None`.
    pub async fn ecu_reset(&self, reset_type: u8) -> Result<Option<u8>> {
        self.client.ecu_reset(reset_type | 0x80).await
    }

    /// Security Access with positive-response suppression.
    /// Silence returns an empty vector; use the original client to obtain a seed.
    pub async fn security_access(&self, access_type: u8, data: Option<&[u8]>) -> Result<Vec<u8>> {
        self.client.security_access(access_type | 0x80, data).await
    }

    /// Tester Present without a positive response (`[0x3e, 0x80]`).
    /// Waits through the initial response window to catch negative responses.
    /// NRC 0x78 requires a final response; missing it returns a timeout.
    pub async fn tester_present(&self) -> Result<()> {
        if let RequestOutcome::PositiveResponse(response) = self
            .client
            .request_exchange(ServiceIdentifier::TesterPresent as u8, Some(0x80), None)
            .await?
        {
            self.client.validate_response_length(response, 2, Some(2))?;
        }
        Ok(())
    }

    /// Routine Control with positive-response suppression. Silence returns `None`.
    pub async fn routine_control(
        &self,
        routine_control_type: RoutineControlType,
        routine_identifier: u16,
        data: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>> {
        self.client
            .routine_control_inner(routine_control_type as u8 | 0x80, routine_identifier, data)
            .await
    }
}
