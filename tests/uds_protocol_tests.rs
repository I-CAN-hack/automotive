//! Scripted transport exchanges exercise UDS wire values without CAN hardware.

use automotive::protocols::uds::{
    CompressionMethod, DTCFormatIdentifier, DataIdentifier, EncryptionMethod, Error as UDSError,
    NegativeResponseCode, ProtocolDeviation, RequestOutcome, ResetType, RoutineControlType,
    RoutineIdentifier, SecurityAccessType, ServiceIdentifier, SessionType, UDSClient, UDSConfig,
};
use automotive::{Error, Stream, TransportLayer};
use std::sync::atomic::{AtomicUsize, Ordering};

struct ScriptedTransport {
    request: Vec<u8>,
    responses: Vec<Vec<u8>>,
    sent: AtomicUsize,
}

impl ScriptedTransport {
    fn new(request: &[u8], response: &[u8]) -> Self {
        Self::with_responses(request, &[response])
    }

    fn with_responses(request: &[u8], responses: &[&[u8]]) -> Self {
        Self {
            request: request.to_vec(),
            responses: responses.iter().map(|response| response.to_vec()).collect(),
            sent: AtomicUsize::new(0),
        }
    }

    fn client(&self, strict: bool) -> UDSClient<'_, Self> {
        UDSClient::with_config(
            self,
            UDSConfig::builder()
                .p2_timeout(std::time::Duration::from_millis(1))
                .strict(strict)
                .use_server_timing(false)
                .build(),
        )
    }
}

impl TransportLayer for ScriptedTransport {
    async fn send<'a>(&'a self, data: &'a [u8]) -> automotive::Result<()> {
        assert_eq!(data, self.request);
        self.sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn recv(&self) -> impl Stream<Item = automotive::Result<Vec<u8>>> + Unpin + '_ {
        tokio_stream::iter(self.responses.iter().cloned().map(Ok))
    }
}

fn assert_deviation(error: Error, expected: ProtocolDeviation, payload: &[u8]) {
    match error {
        Error::UDSError(UDSError::ProtocolDeviation {
            deviation,
            response,
        }) => {
            assert_eq!(deviation, expected);
            assert_eq!(response, payload);
        }
        other => panic!("Expected protocol deviation, received {other:?}"),
    }
}

#[tokio::test]
async fn reset_arguments_preserve_wire_values_and_power_down_semantics() {
    for strict in [false, true] {
        let transport = ScriptedTransport::new(&[0x11, 0x01], &[0x51, 0x01]);
        let uds = transport.client(strict);
        assert_eq!(uds.ecu_reset(ResetType::HardReset).await.unwrap(), None);
        assert_eq!(uds.ecu_reset(0x1).await.unwrap(), None);
        assert_eq!(
            uds.ecu_reset(ResetType::NonStandard(0x01)).await.unwrap(),
            None
        );

        let transport = ScriptedTransport::new(&[0x11, 0x04], &[0x51, 0x04, 9]);
        let uds = transport.client(strict);
        assert_eq!(
            uds.ecu_reset(ResetType::EnableRapidPowerShutDown)
                .await
                .unwrap(),
            Some(9)
        );
        assert_eq!(uds.ecu_reset(0x4).await.unwrap(), Some(9));
        assert_eq!(
            uds.ecu_reset(ResetType::NonStandard(0x04)).await.unwrap(),
            Some(9)
        );

        // The request retains the suppression bit; the response does not echo it.
        let transport = ScriptedTransport::with_responses(
            &[0x11, 0x84],
            &[&[0x7f, 0x11, 0x78], &[0x51, 0x04, 9]],
        );
        let uds = transport.client(strict);
        assert_eq!(uds.ecu_reset(0x84).await.unwrap(), Some(9));
        assert_eq!(
            uds.ecu_reset(ResetType::NonStandard(0x84)).await.unwrap(),
            Some(9)
        );

        let transport = ScriptedTransport::new(&[0x11, 0x41], &[0x51, 0x41]);
        let uds = transport.client(strict);
        assert_eq!(uds.ecu_reset(0x41).await.unwrap(), None);
        assert_eq!(
            uds.ecu_reset(ResetType::NonStandard(0x41)).await.unwrap(),
            None
        );
    }
}

#[tokio::test]
async fn session_arguments_encode_named_and_custom_sessions() {
    for strict in [false, true] {
        let transport = ScriptedTransport::new(&[0x10, 0x03], &[0x50, 0x03, 0, 50, 0, 50]);
        let uds = transport.client(strict);
        let timing = uds
            .diagnostic_session_control(SessionType::ExtendedDiagnostic)
            .await
            .unwrap();
        assert_eq!(uds.diagnostic_session_control(0x3).await.unwrap(), timing);
        assert_eq!(
            uds.diagnostic_session_control(SessionType::NonStandard(0x03))
                .await
                .unwrap(),
            timing
        );

        let transport = ScriptedTransport::new(&[0x10, 0x41], &[0x50, 0x41, 0, 50, 0, 50]);
        let uds = transport.client(strict);
        assert_eq!(uds.diagnostic_session_control(0x41).await.unwrap(), timing);
        assert_eq!(
            uds.diagnostic_session_control(SessionType::NonStandard(0x41))
                .await
                .unwrap(),
            timing
        );
    }
}

#[tokio::test]
async fn security_access_arguments_preserve_seed_and_key_parity() {
    for strict in [false, true] {
        let transport = ScriptedTransport::new(&[0x27, 0x01], &[0x67, 0x01, 0xaa]);
        let uds = transport.client(strict);
        assert_eq!(
            uds.security_access(SecurityAccessType::RequestSeed, None)
                .await
                .unwrap(),
            Some(vec![0xaa])
        );
        assert_eq!(
            uds.security_access(0x1, None).await.unwrap(),
            Some(vec![0xaa])
        );

        let transport = ScriptedTransport::new(&[0x27, 0x11], &[0x67, 0x11, 0xaa]);
        let uds = transport.client(strict);
        assert_eq!(
            uds.security_access(0x11, None).await.unwrap(),
            Some(vec![0xaa])
        );
        assert_eq!(
            uds.security_access(SecurityAccessType::NonStandard(0x11), None)
                .await
                .unwrap(),
            Some(vec![0xaa])
        );

        let transport = ScriptedTransport::new(&[0x27, 0x12, 0xbb], &[0x67, 0x12]);
        let uds = transport.client(strict);
        assert_eq!(
            uds.security_access(0x12, Some(&[0xbb])).await.unwrap(),
            None
        );
        assert_eq!(
            uds.security_access(SecurityAccessType::NonStandard(0x12), Some(&[0xbb]))
                .await
                .unwrap(),
            None
        );
    }
}

#[tokio::test]
async fn data_identifiers_encode_big_endian_and_validate_numeric_echoes() {
    for strict in [false, true] {
        let transport = ScriptedTransport::new(&[0x22, 0xf1, 0x90], &[0x62, 0xf1, 0x90, 0xaa]);
        let uds = transport.client(strict);
        assert_eq!(
            uds.read_data_by_identifier(DataIdentifier::Vin)
                .await
                .unwrap(),
            [0xaa]
        );
        assert_eq!(uds.read_data_by_identifier(0xf190).await.unwrap(), [0xaa]);
        assert_eq!(
            uds.read_data_by_identifier(DataIdentifier::NonStandard(0xf190))
                .await
                .unwrap(),
            [0xaa]
        );

        let transport = ScriptedTransport::new(&[0x2e, 0xf1, 0x90, 0xbb], &[0x6e, 0xf1, 0x90]);
        let uds = transport.client(strict);
        uds.write_data_by_identifier(DataIdentifier::Vin, &[0xbb])
            .await
            .unwrap();
        uds.write_data_by_identifier(0xf190, &[0xbb]).await.unwrap();
        uds.write_data_by_identifier(DataIdentifier::NonStandard(0xf190), &[0xbb])
            .await
            .unwrap();

        let transport = ScriptedTransport::new(&[0x22, 0x12, 0x34], &[0x62, 0x12, 0x34, 0xaa]);
        let uds = transport.client(strict);
        assert_eq!(uds.read_data_by_identifier(0x1234).await.unwrap(), [0xaa]);
        assert_eq!(
            uds.read_data_by_identifier(DataIdentifier::NonStandard(0x1234))
                .await
                .unwrap(),
            [0xaa]
        );

        let transport = ScriptedTransport::new(&[0x2e, 0x12, 0x34, 0xbb], &[0x6e, 0x12, 0x34]);
        let uds = transport.client(strict);
        uds.write_data_by_identifier(0x1234, &[0xbb]).await.unwrap();
        uds.write_data_by_identifier(DataIdentifier::NonStandard(0x1234), &[0xbb])
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn routine_arguments_encode_named_and_custom_operations_and_identifiers() {
    for strict in [false, true] {
        let transport =
            ScriptedTransport::new(&[0x31, 0x01, 0xff, 0x00], &[0x71, 0x01, 0xff, 0x00]);
        let uds = transport.client(strict);
        assert_eq!(
            uds.routine_control(
                RoutineControlType::Start,
                RoutineIdentifier::EraseMemory,
                None
            )
            .await
            .unwrap(),
            None
        );
        assert_eq!(uds.routine_control(0x1, 0xff00, None).await.unwrap(), None);
        assert_eq!(
            uds.routine_control(
                RoutineControlType::NonStandard(0x01),
                RoutineIdentifier::NonStandard(0xff00),
                None,
            )
            .await
            .unwrap(),
            None
        );

        let transport = ScriptedTransport::new(
            &[0x31, 0x41, 0x12, 0x34, 0xaa],
            &[0x71, 0x41, 0x12, 0x34, 0xbb],
        );
        let uds = transport.client(strict);
        assert_eq!(
            uds.routine_control(0x41, 0x1234, Some(&[0xaa]))
                .await
                .unwrap(),
            Some(vec![0xbb])
        );
        assert_eq!(
            uds.routine_control(
                RoutineControlType::NonStandard(0x41),
                RoutineIdentifier::NonStandard(0x1234),
                Some(&[0xaa]),
            )
            .await
            .unwrap(),
            Some(vec![0xbb])
        );
    }
}

#[tokio::test]
async fn custom_requests_accept_typed_and_raw_service_identifiers() {
    for strict in [false, true] {
        let transport = ScriptedTransport::new(&[0x3e, 0x00], &[0x7e, 0x00]);
        let uds = transport.client(strict);
        assert_eq!(
            uds.request(ServiceIdentifier::TesterPresent, Some(0), None)
                .await
                .unwrap(),
            RequestOutcome::PositiveResponse(vec![])
        );
        assert_eq!(
            uds.request(0x3e, Some(0), None).await.unwrap(),
            RequestOutcome::PositiveResponse(vec![])
        );
        assert_eq!(
            uds.request(ServiceIdentifier::NonStandard(0x3e), Some(0), None)
                .await
                .unwrap(),
            RequestOutcome::PositiveResponse(vec![])
        );

        let transport = ScriptedTransport::new(&[0x29, 0xaa], &[0x69, 0xbb]);
        let uds = transport.client(strict);
        assert_eq!(
            uds.request(0x29, None, Some(&[0xaa])).await.unwrap(),
            RequestOutcome::PositiveResponse(vec![0xbb])
        );
        assert_eq!(
            uds.request(ServiceIdentifier::NonStandard(0x29), None, Some(&[0xaa]))
                .await
                .unwrap(),
            RequestOutcome::PositiveResponse(vec![0xbb])
        );
    }
}

#[tokio::test]
async fn suppressed_helpers_accept_typed_raw_and_custom_arguments() {
    for strict in [false, true] {
        let transport =
            ScriptedTransport::with_responses(&[0x11, 0x81], &[&[0x7f, 0x11, 0x78], &[0x51, 0x01]]);
        let uds = transport.client(strict);
        let suppressed = uds.suppress_positive_responses();
        assert_eq!(
            suppressed.ecu_reset(ResetType::HardReset).await.unwrap(),
            None
        );
        assert_eq!(suppressed.ecu_reset(0x1).await.unwrap(), None);
        assert_eq!(
            suppressed
                .ecu_reset(ResetType::NonStandard(0x01))
                .await
                .unwrap(),
            None
        );

        let transport = ScriptedTransport::with_responses(
            &[0x10, 0x83],
            &[&[0x7f, 0x10, 0x78], &[0x50, 0x03, 0, 50, 0, 50]],
        );
        let uds = transport.client(strict);
        let suppressed = uds.suppress_positive_responses();
        let timing = suppressed
            .diagnostic_session_control(SessionType::ExtendedDiagnostic)
            .await
            .unwrap();
        assert_eq!(
            suppressed.diagnostic_session_control(0x3).await.unwrap(),
            timing
        );
        assert_eq!(
            suppressed
                .diagnostic_session_control(SessionType::NonStandard(0x03))
                .await
                .unwrap(),
            timing
        );

        let transport = ScriptedTransport::with_responses(
            &[0x27, 0x81],
            &[&[0x7f, 0x27, 0x78], &[0x67, 0x01, 0xaa]],
        );
        let uds = transport.client(strict);
        let suppressed = uds.suppress_positive_responses();
        assert_eq!(
            suppressed
                .security_access(SecurityAccessType::RequestSeed, None)
                .await
                .unwrap(),
            Some(vec![0xaa])
        );
        assert_eq!(
            suppressed.security_access(0x1, None).await.unwrap(),
            Some(vec![0xaa])
        );

        let transport = ScriptedTransport::with_responses(
            &[0x27, 0x91],
            &[&[0x7f, 0x27, 0x78], &[0x67, 0x11, 0xaa]],
        );
        let uds = transport.client(strict);
        let suppressed = uds.suppress_positive_responses();
        assert_eq!(
            suppressed
                .security_access(SecurityAccessType::NonStandard(0x11), None)
                .await
                .unwrap(),
            Some(vec![0xaa])
        );
        assert_eq!(
            suppressed.security_access(0x11, None).await.unwrap(),
            Some(vec![0xaa])
        );

        let transport = ScriptedTransport::with_responses(
            &[0x31, 0x81, 0xff, 0x00],
            &[&[0x7f, 0x31, 0x78], &[0x71, 0x01, 0xff, 0x00]],
        );
        let uds = transport.client(strict);
        let suppressed = uds.suppress_positive_responses();
        assert_eq!(
            suppressed
                .routine_control(
                    RoutineControlType::Start,
                    RoutineIdentifier::EraseMemory,
                    None
                )
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            suppressed.routine_control(0x1, 0xff00, None).await.unwrap(),
            None
        );
        assert_eq!(
            suppressed
                .routine_control(
                    RoutineControlType::NonStandard(0x01),
                    RoutineIdentifier::NonStandard(0xff00),
                    None
                )
                .await
                .unwrap(),
            None
        );

        let transport =
            ScriptedTransport::with_responses(&[0x3e, 0x80], &[&[0x7f, 0x3e, 0x78], &[0x7e, 0x00]]);
        let uds = transport.client(strict);
        let suppressed = uds.suppress_positive_responses();
        assert_eq!(
            suppressed
                .request(ServiceIdentifier::TesterPresent, Some(0), None)
                .await
                .unwrap(),
            RequestOutcome::PositiveResponse(vec![])
        );
        assert_eq!(
            suppressed.request(0x3e, Some(0), None).await.unwrap(),
            RequestOutcome::PositiveResponse(vec![])
        );
    }
}

#[tokio::test]
async fn unknown_response_codes_and_dtc_formats_are_preserved_in_both_modes() {
    for strict in [false, true] {
        for code in [0x00, 0x15, 0x38, 0x80, 0xff] {
            let transport = ScriptedTransport::new(&[0x11, 0x01], &[0x7f, 0x11, code]);
            assert!(matches!(
                transport.client(strict).ecu_reset(0x1).await,
                Err(Error::UDSError(UDSError::NegativeResponse(NegativeResponseCode::NonStandard(value)))) if value == code
            ));
        }

        let transport =
            ScriptedTransport::new(&[0x19, 0x01, 0xff], &[0x59, 0x01, 0xaa, 0xf1, 0x12, 0x34]);
        let report = transport
            .client(strict)
            .read_dtc_information_number_of_dtc_by_status_mask(0xff)
            .await
            .unwrap();
        assert_eq!(report.dtc_status_availability_mask, 0xaa);
        assert_eq!(
            report.dtc_format_identifier,
            DTCFormatIdentifier::NonStandard(0xf1)
        );
        assert_eq!(report.dtc_count, 0x1234);
    }
}

#[tokio::test]
async fn response_pending_still_waits_for_the_final_response() {
    for strict in [false, true] {
        let transport =
            ScriptedTransport::with_responses(&[0x11, 0x01], &[&[0x7f, 0x11, 0x78], &[0x51, 0x01]]);
        assert_eq!(transport.client(strict).ecu_reset(0x1).await.unwrap(), None);
        assert_eq!(transport.sent.load(Ordering::Relaxed), 1);
    }
}

#[tokio::test]
async fn protocol_errors_preserve_typed_identifiers_and_full_responses() {
    let payload = [0x52, 0x01];
    let transport = ScriptedTransport::new(&[0x11, 0x01], &payload);
    let error = transport.client(true).ecu_reset(0x1).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "Protocol deviation: Expected response service ID 0x51, received 0x52; response: [52, 01]"
    );
    assert_deviation(
        error,
        ProtocolDeviation::InvalidServiceId {
            expected: ServiceIdentifier::NonStandard(0x51),
            actual: ServiceIdentifier::NonStandard(0x52),
        },
        &payload,
    );

    let payload = [0x7f, 0x29, 0xee];
    let transport = ScriptedTransport::new(&[0x11, 0x01], &payload);
    assert_deviation(
        transport.client(true).ecu_reset(0x1).await.unwrap_err(),
        ProtocolDeviation::InvalidRequestServiceId {
            expected: ServiceIdentifier::EcuReset,
            actual: ServiceIdentifier::NonStandard(0x29),
        },
        &payload,
    );

    let payload = [0x62, 0x12, 0x34, 0xaa];
    let transport = ScriptedTransport::new(&[0x22, 0xf1, 0x90], &payload);
    assert_deviation(
        transport
            .client(true)
            .read_data_by_identifier(DataIdentifier::Vin)
            .await
            .unwrap_err(),
        ProtocolDeviation::InvalidDataIdentifier {
            expected: DataIdentifier::Vin,
            actual: DataIdentifier::NonStandard(0x1234),
        },
        &payload,
    );

    let payload = [0x6e, 0xf1, 0x90];
    let transport = ScriptedTransport::new(&[0x2e, 0x12, 0x34, 0xaa], &payload);
    assert_deviation(
        transport
            .client(true)
            .write_data_by_identifier(0x1234, &[0xaa])
            .await
            .unwrap_err(),
        ProtocolDeviation::InvalidDataIdentifier {
            expected: DataIdentifier::NonStandard(0x1234),
            actual: DataIdentifier::Vin,
        },
        &payload,
    );

    let payload = [0x71, 0x01, 0x12, 0x34];
    let transport = ScriptedTransport::new(&[0x31, 0x01, 0xff, 0x00], &payload);
    assert_deviation(
        transport
            .client(true)
            .routine_control(0x1, RoutineIdentifier::EraseMemory, None)
            .await
            .unwrap_err(),
        ProtocolDeviation::InvalidRoutineIdentifier {
            expected: RoutineIdentifier::EraseMemory,
            actual: RoutineIdentifier::NonStandard(0x1234),
        },
        &payload,
    );
}

#[tokio::test]
async fn unknown_values_do_not_disable_structural_validation() {
    let payload = [0x7f, 0x11, 0xff, 0xaa];
    let transport = ScriptedTransport::new(&[0x11, 0x01], &payload);
    assert_deviation(
        transport.client(true).ecu_reset(0x1).await.unwrap_err(),
        ProtocolDeviation::InvalidResponseLength {
            minimum: 3,
            maximum: Some(3),
        },
        &payload,
    );

    for strict in [false, true] {
        let transport =
            ScriptedTransport::new(&[0x19, 0x01, 0xff], &[0x59, 0x01, 0xaa, 0xf1, 0x12]);
        assert!(transport
            .client(strict)
            .read_dtc_information_number_of_dtc_by_status_mask(0xff)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn transfer_methods_pack_separate_nibbles_for_typed_raw_and_mixed_inputs() {
    for (compression, encryption, wire) in [
        (0, 0, 0x00),
        (0xa, 0x3, 0xa3),
        (0xf, 0, 0xf0),
        (0, 0xf, 0x0f),
        (0xf, 0xf, 0xff),
    ] {
        let typed_compression = CompressionMethod::new(compression).unwrap();
        let typed_encryption = EncryptionMethod::new(encryption).unwrap();
        let transport =
            ScriptedTransport::new(&[0x34, wire, 0x11, 0x12, 0x10], &[0x74, 0x10, 0x10]);
        let uds = transport.client(true);
        assert_eq!(
            uds.request_download(compression, encryption, &[0x12], &[0x10])
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            uds.request_download(typed_compression, typed_encryption, &[0x12], &[0x10])
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            uds.request_download(typed_compression, encryption, &[0x12], &[0x10])
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            uds.request_download(compression, typed_encryption, &[0x12], &[0x10])
                .await
                .unwrap(),
            16
        );

        let transport =
            ScriptedTransport::new(&[0x35, wire, 0x11, 0x12, 0x10], &[0x75, 0x10, 0x10]);
        let uds = transport.client(true);
        assert_eq!(
            uds.request_upload(compression, encryption, &[0x12], &[0x10])
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            uds.request_upload(typed_compression, typed_encryption, &[0x12], &[0x10])
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            uds.request_upload(typed_compression, encryption, &[0x12], &[0x10])
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            uds.request_upload(compression, typed_encryption, &[0x12], &[0x10])
                .await
                .unwrap(),
            16
        );
    }
}

#[tokio::test]
async fn invalid_transfer_methods_return_errors_before_transmitting() {
    for strict in [false, true] {
        let transport = ScriptedTransport::new(&[], &[]);
        let uds = transport.client(strict);
        for invalid in [0x10, 0xff] {
            assert!(
                matches!(uds.request_download(invalid, 0, &[0x12], &[0x10]).await,
                Err(Error::UDSError(UDSError::InvalidCompressionMethod(value))) if value == invalid)
            );
            assert!(
                matches!(uds.request_download(CompressionMethod::NONE, invalid, &[0x12], &[0x10]).await,
                Err(Error::UDSError(UDSError::InvalidEncryptionMethod(value))) if value == invalid)
            );
            assert!(
                matches!(uds.request_upload(invalid, EncryptionMethod::NONE, &[0x12], &[0x10]).await,
                Err(Error::UDSError(UDSError::InvalidCompressionMethod(value))) if value == invalid)
            );
            assert!(
                matches!(uds.request_upload(0, invalid, &[0x12], &[0x10]).await,
                Err(Error::UDSError(UDSError::InvalidEncryptionMethod(value))) if value == invalid)
            );
        }
        assert_eq!(transport.sent.load(Ordering::Relaxed), 0);
    }
}
