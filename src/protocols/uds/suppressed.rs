//! Scoped access to services with positive-response suppression enabled.

use super::{
    DTCReportNumberByStatusMask, ReportType, RequestOutcome, ResetType, RoutineControlType,
    RoutineIdentifier, SecurityAccessType, ServiceIdentifier, SessionParameterRecord, SessionType,
    UDSClient,
};
use crate::{Result, TransportLayer};
use std::ops::Deref;

/// A borrowed view of a [`UDSClient`] that suppresses positive responses.
///
/// Create it with [`UDSClient::suppress_positive_responses`]. The view shares
/// transport and session timings with the original client. Helpers for services
/// without a sub-function are available through [`Deref`] and require a response.
pub struct SuppressedUDSClient<'client, 'adapter, T: TransportLayer> {
    pub(super) client: &'client UDSClient<'adapter, T>,
}

impl<'adapter, T: TransportLayer> Deref for SuppressedUDSClient<'_, 'adapter, T> {
    type Target = UDSClient<'adapter, T>;

    fn deref(&self) -> &Self::Target {
        self.client
    }
}

impl<T: TransportLayer> SuppressedUDSClient<'_, '_, T> {
    /// Make a custom request with bit 7 set in its sub-function byte.
    /// Requests without a sub-function still require a response.
    pub async fn request(
        &self,
        sid: impl Into<ServiceIdentifier>,
        sub_function: Option<u8>,
        data: Option<&[u8]>,
    ) -> Result<RequestOutcome<Vec<u8>>> {
        self.client
            .request(sid, sub_function.map(|value| value | 0x80), data)
            .await
    }

    /// 0x10 - Diagnostic Session Control.
    pub async fn diagnostic_session_control(
        &self,
        session_type: impl Into<SessionType>,
    ) -> Result<Option<SessionParameterRecord>> {
        let session_type: SessionType = session_type.into();
        self.client
            .diagnostic_session_control(session_type.as_raw() | 0x80)
            .await
    }

    /// 0x11 - ECU Reset.
    pub async fn ecu_reset(&self, reset_type: impl Into<ResetType>) -> Result<Option<u8>> {
        let reset_type: ResetType = reset_type.into();
        self.client.ecu_reset(reset_type.as_raw() | 0x80).await
    }

    /// 0x27 - Security Access. Returns a received seed, or `None` for SendKey
    /// and suppressed silence.
    pub async fn security_access(
        &self,
        access_type: impl Into<SecurityAccessType>,
        data: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>> {
        let access_type: SecurityAccessType = access_type.into();
        self.client
            .security_access(access_type.as_raw() | 0x80, data)
            .await
    }

    /// 0x3E - Tester Present.
    pub async fn tester_present(&self) -> Result<()> {
        if let RequestOutcome::PositiveResponse(response) = self
            .client
            .request_exchange(ServiceIdentifier::TesterPresent, Some(0x80), None)
            .await?
        {
            self.client.validate_response_length(response, 2, Some(2))?;
        }
        Ok(())
    }

    /// 0x19, sub-function 0x01 - Report the number of DTCs matching `mask`.
    /// Returns `None` for suppressed silence and `Some(report)` for a received report.
    pub async fn read_dtc_information_number_of_dtc_by_status_mask(
        &self,
        mask: u8,
    ) -> Result<Option<DTCReportNumberByStatusMask>> {
        match self
            .client
            .request_exchange(
                ServiceIdentifier::ReadDTCInformation,
                Some(ReportType::ReportNumberOfDTCByStatusMask.as_raw() | 0x80),
                Some(&[mask]),
            )
            .await?
        {
            RequestOutcome::PositiveResponse(response) => Ok(Some(
                self.client
                    .parse_dtc_report_number_by_status_mask(response)?,
            )),
            RequestOutcome::Suppressed => Ok(None),
        }
    }

    /// 0x31 - Routine Control.
    pub async fn routine_control(
        &self,
        routine_control_type: impl Into<RoutineControlType>,
        routine_identifier: impl Into<RoutineIdentifier>,
        data: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>> {
        let routine_control_type: RoutineControlType = routine_control_type.into();
        self.client
            .routine_control(
                routine_control_type.as_raw() | 0x80,
                routine_identifier,
                data,
            )
            .await
    }
}
