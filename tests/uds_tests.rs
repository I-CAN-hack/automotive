#![cfg(all(target_os = "linux", feature = "test-vcan"))]

use automotive::adapters::socketcan::SocketCanConfig;
use automotive::can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::protocols::uds::Error as UDSError;
use automotive::protocols::uds::NegativeResponseCode;
use automotive::protocols::uds::{
    DTCFormatIdentifier, ProtocolDeviation, RequestOutcome, RoutineControlType, ServiceIdentifier,
    UDSClient, UDSConfig,
};
use automotive::transports::isotp::{IsoTP, IsoTPConfig};
use automotive::Error;
use automotive::StreamExt;
use std::process::{Child, Command};
use std::time::Duration;

#[path = "common/uds.rs"]
mod uds_services;

static VECU_STARTUP_TIMEOUT_MS: u64 = 10000;

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn vecu_spawn(adapter: &AsyncCanAdapter) -> ChildGuard {
    vecu_spawn_with_case(adapter, None).await
}

async fn vecu_spawn_with_case(adapter: &AsyncCanAdapter, case: Option<&str>) -> ChildGuard {
    let stream = adapter
        .recv()
        .timeout(std::time::Duration::from_millis(VECU_STARTUP_TIMEOUT_MS));
    tokio::pin!(stream);

    let mut command = Command::new("scripts/vecu_uds.py");
    if let Some(case) = case {
        command.args(["--suppression-case", case]);
    }
    let vecu = ChildGuard(command.spawn().unwrap());
    stream.next().await.unwrap().expect("vecu did not start");

    vecu
}

fn suppression_config(strict: bool) -> UDSConfig {
    UDSConfig::builder()
        .strict(strict)
        .p2_timeout(Duration::from_millis(100))
        .p2_star_timeout(Duration::from_millis(200))
        .use_server_timing(false)
        .build()
}

fn vcan_adapter() -> AsyncCanAdapter {
    automotive::adapters::socketcan::SocketCanAdapter::new_async(SocketCanConfig::new("vcan0"))
        .unwrap()
}

fn suppression_transport(adapter: &AsyncCanAdapter) -> IsoTP<'_> {
    let config = IsoTPConfig::builder(Identifier::standard(0x7a1).unwrap())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    IsoTP::new(adapter, config)
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
#[serial_test::serial]
async fn uds_test_sids() {
    let adapter =
        automotive::adapters::socketcan::SocketCanAdapter::new_async(SocketCanConfig::new("vcan0"))
            .unwrap();
    let _vecu = vecu_spawn(&adapter).await;

    let isotp_config = IsoTPConfig::builder(Identifier::standard(0x7a1).unwrap())
        .timeout(std::time::Duration::from_millis(1000))
        .build()
        .unwrap();

    let isotp = IsoTP::new(&adapter, isotp_config);
    let uds_config = UDSConfig::builder()
        .p2_timeout(Duration::from_secs(1))
        .build();
    let uds = UDSClient::with_config(&isotp, uds_config);

    uds_services::exercise_services(&uds).await;
}

#[tokio::test]
#[serial_test::serial]
async fn uds_suppressed_services_accept_silence_in_both_modes() {
    for strict in [true, false] {
        let adapter = vcan_adapter();
        let _vecu = vecu_spawn(&adapter).await;
        let transport = suppression_transport(&adapter);
        let uds = UDSClient::with_config(&transport, suppression_config(strict));
        let suppressed = uds.suppress_positive_responses();

        let start = tokio::time::Instant::now();
        assert_eq!(
            uds.request(0x3e, Some(0x80), None).await.unwrap(),
            RequestOutcome::Suppressed
        );
        assert!(start.elapsed() >= Duration::from_millis(100));
        suppressed.tester_present().await.unwrap();
        assert_eq!(
            suppressed.diagnostic_session_control(3).await.unwrap(),
            None
        );
        assert_eq!(
            uds.request(0x10, Some(0x83), None).await.unwrap(),
            RequestOutcome::Suppressed
        );
        assert_eq!(suppressed.ecu_reset(4).await.unwrap(), None);
        assert_eq!(suppressed.security_access(1, None).await.unwrap(), None);
        assert_eq!(
            suppressed.security_access(2, Some(b"key!")).await.unwrap(),
            None
        );
        assert_eq!(
            suppressed
                .read_dtc_information_number_of_dtc_by_status_mask(0xff)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            suppressed
                .routine_control(RoutineControlType::Stop, 0x1234, None)
                .await
                .unwrap(),
            None
        );
        // The normal ECU sends NRC 0x78 before completing this routine.
        assert_eq!(
            suppressed
                .routine_control(RoutineControlType::Start, 0x1234, None)
                .await
                .unwrap(),
            Some(vec![0])
        );
        assert!(matches!(
            suppressed.security_access(2, Some(b"bad!")).await,
            Err(Error::UDSError(UDSError::NegativeResponse(
                NegativeResponseCode::InvalidKey
            )))
        ));
        // The original client and requests without a sub-function still receive replies.
        uds.tester_present().await.unwrap();
        assert_eq!(
            suppressed.read_data_by_identifier(0x1234).await.unwrap(),
            b"deadbeef"
        );
        assert_eq!(
            suppressed
                .request(0x22, None, Some(&[0x12, 0x34]))
                .await
                .unwrap(),
            RequestOutcome::PositiveResponse(b"\x12\x34deadbeef".to_vec())
        );
    }
}

#[tokio::test]
#[serial_test::serial]
async fn uds_suppressed_pending_replies_require_a_final_response() {
    for strict in [true, false] {
        for case in [
            "pending-positive",
            "pending-negative",
            "pending-only",
            "repeated-pending",
        ] {
            let adapter = vcan_adapter();
            let _vecu = vecu_spawn_with_case(&adapter, Some(case)).await;
            let transport = suppression_transport(&adapter);
            let mut config = suppression_config(strict);
            if case == "pending-only" {
                // Pending state must not be inferred from different timer values.
                config.p2_star_timeout = config.p2_timeout;
            }
            let uds = UDSClient::with_config(&transport, config);
            let start = tokio::time::Instant::now();
            let result = uds.request(0x3e, Some(0x80), None).await;
            match case {
                "pending-only" => assert!(matches!(result, Err(Error::Timeout))),
                "pending-negative" => assert!(matches!(
                    result,
                    Err(Error::UDSError(UDSError::NegativeResponse(
                        NegativeResponseCode::SecurityAccessDenied
                    )))
                )),
                _ => assert_eq!(result.unwrap(), RequestOutcome::PositiveResponse(vec![])),
            }
            if case == "repeated-pending" {
                assert!(start.elapsed() > Duration::from_millis(200));
            }
            if case == "pending-positive" {
                assert_eq!(
                    uds.security_access(0x81, None).await.unwrap(),
                    Some(b"seed".to_vec())
                );
                assert_eq!(
                    uds.suppress_positive_responses()
                        .security_access(2, Some(b"key!"))
                        .await
                        .unwrap(),
                    None
                );
                let report = uds
                    .suppress_positive_responses()
                    .read_dtc_information_number_of_dtc_by_status_mask(0xff)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(report.dtc_status_availability_mask, 0xff);
                assert_eq!(
                    report.dtc_format_identifier,
                    DTCFormatIdentifier::ISO_14229_1_DTCFormat
                );
                assert_eq!(report.dtc_count, 2);
            }
        }
    }
}

#[tokio::test]
#[serial_test::serial]
async fn uds_strict_suppression_rejects_unexpected_positive_replies_and_bad_echoes() {
    for case in ["positive", "bad-echo"] {
        let adapter = vcan_adapter();
        let _vecu = vecu_spawn_with_case(&adapter, Some(case)).await;
        let transport = suppression_transport(&adapter);
        for strict in [true, false] {
            let uds = UDSClient::with_config(&transport, suppression_config(strict));
            for use_view in [false, true] {
                let result = if use_view {
                    uds.suppress_positive_responses()
                        .request(0x3e, Some(0), None)
                        .await
                } else {
                    uds.request(0x3e, Some(0x80), None).await
                };
                if strict {
                    let (deviation, payload) = if case == "positive" {
                        (ProtocolDeviation::UnexpectedPositiveResponse, vec![0x7e, 0])
                    } else {
                        (
                            ProtocolDeviation::InvalidSubFunction {
                                expected: 0,
                                actual: 0x80,
                            },
                            vec![0x7e, 0x80],
                        )
                    };
                    assert_deviation(result.unwrap_err(), deviation, &payload);
                } else {
                    assert_eq!(result.unwrap(), RequestOutcome::PositiveResponse(vec![]));
                }
            }
            uds.tester_present().await.unwrap();
        }
    }
}

#[tokio::test]
#[serial_test::serial]
async fn uds_suppressed_final_responses_still_validate_service_records() {
    let adapter = vcan_adapter();
    let _vecu = vecu_spawn_with_case(&adapter, Some("malformed-final")).await;
    let transport = suppression_transport(&adapter);
    for strict in [true, false] {
        let uds = UDSClient::with_config(&transport, suppression_config(strict));
        let suppressed = uds.suppress_positive_responses();
        let results = [
            suppressed.diagnostic_session_control(3).await.map(|_| ()),
            suppressed.ecu_reset(4).await.map(|_| ()),
            suppressed.security_access(1, None).await.map(|_| ()),
            suppressed.tester_present().await,
        ];
        for ((payload, minimum, maximum), result) in [
            (vec![0x50, 3], 6, Some(6)),
            (vec![0x51, 4], 3, Some(3)),
            (vec![0x67, 1], 3, None),
            (vec![0x7e, 0, 0xaa], 2, Some(2)),
        ]
        .into_iter()
        .zip(results)
        {
            if strict {
                assert_deviation(
                    result.unwrap_err(),
                    ProtocolDeviation::InvalidResponseLength { minimum, maximum },
                    &payload,
                );
            } else {
                result.unwrap();
            }
        }
        // These records are necessary for safe parsing even in permissive mode.
        let errors = [
            suppressed
                .routine_control(RoutineControlType::Start, 0x1234, None)
                .await
                .unwrap_err(),
            suppressed
                .read_dtc_information_number_of_dtc_by_status_mask(0xff)
                .await
                .unwrap_err(),
        ];
        for ((payload, minimum, maximum), error) in
            [(vec![0x71, 1], 4, None), (vec![0x59, 1], 6, Some(6))]
                .into_iter()
                .zip(errors)
        {
            if strict {
                assert_deviation(
                    error,
                    ProtocolDeviation::InvalidResponseLength { minimum, maximum },
                    &payload,
                );
            } else {
                assert!(matches!(
                    error,
                    Error::UDSError(UDSError::InvalidResponseLength)
                ));
            }
        }
    }
}

#[tokio::test]
#[serial_test::serial]
async fn uds_suppressed_pending_pdus_are_validated_before_extending_the_wait() {
    for (case, payload, deviation) in [
        (
            "wrong-pending-sid",
            vec![0x7f, 0x3f, 0x78],
            ProtocolDeviation::InvalidRequestServiceId {
                expected: ServiceIdentifier::TesterPresent,
                actual: ServiceIdentifier::NonStandard(0x3f),
            },
        ),
        (
            "extra-pending-data",
            vec![0x7f, 0x3e, 0x78, 0xaa],
            ProtocolDeviation::InvalidResponseLength {
                minimum: 3,
                maximum: Some(3),
            },
        ),
    ] {
        let adapter = vcan_adapter();
        let _vecu = vecu_spawn_with_case(&adapter, Some(case)).await;
        let transport = suppression_transport(&adapter);
        for strict in [true, false] {
            let uds = UDSClient::with_config(&transport, suppression_config(strict));
            let result = uds.suppress_positive_responses().tester_present().await;
            if strict {
                assert_deviation(result.unwrap_err(), deviation.clone(), &payload);
            } else {
                assert!(matches!(result, Err(Error::Timeout)));
            }
        }
    }
}

#[tokio::test]
#[serial_test::serial]
async fn uds_suppression_preserves_required_and_overall_timeouts() {
    for case in ["silent", "pending-only"] {
        let adapter = vcan_adapter();
        let _vecu = vecu_spawn_with_case(&adapter, Some(case)).await;
        let transport = suppression_transport(&adapter);
        let config = suppression_config(true);
        let uds = UDSClient::with_config(&transport, config);
        if case == "silent" {
            uds.suppress_positive_responses()
                .tester_present()
                .await
                .unwrap();
            assert!(matches!(uds.tester_present().await, Err(Error::Timeout)));
            assert_eq!(
                uds.suppress_positive_responses()
                    .read_dtc_information_number_of_dtc_by_status_mask(0xff)
                    .await
                    .unwrap(),
                None
            );
            assert!(matches!(
                uds.read_dtc_information_number_of_dtc_by_status_mask(0xff)
                    .await,
                Err(Error::Timeout)
            ));
            assert!(matches!(
                uds.suppress_positive_responses()
                    .request(0x22, None, Some(&[0x12, 0x34]))
                    .await,
                Err(Error::Timeout)
            ));
        }
        let config = UDSConfig {
            request_timeout: Some(Duration::from_millis(50)),
            ..config
        };
        let uds = UDSClient::with_config(&transport, config);
        assert!(matches!(
            uds.suppress_positive_responses().tester_present().await,
            Err(Error::Timeout)
        ));
    }
}

#[tokio::test]
#[serial_test::serial]
async fn uds_suppressed_session_silence_preserves_shared_server_timings() {
    let adapter = vcan_adapter();
    let _vecu = vecu_spawn(&adapter).await;
    let transport = suppression_transport(&adapter);
    let uds = UDSClient::with_config(
        &transport,
        UDSConfig::builder()
            .p2_timeout(Duration::from_millis(100))
            .build(),
    );
    let suppressed = uds.suppress_positive_responses();
    let timing = uds.diagnostic_session_control(3).await.unwrap().unwrap();
    assert_eq!(timing.p2_server_max, Duration::from_secs(1));

    let start = tokio::time::Instant::now();
    assert_eq!(
        suppressed.diagnostic_session_control(3).await.unwrap(),
        None
    );
    assert!(start.elapsed() >= timing.p2_server_max);
    let start = tokio::time::Instant::now();
    suppressed.tester_present().await.unwrap();
    assert!(start.elapsed() >= timing.p2_server_max);
}

#[tokio::test]
#[serial_test::serial]
async fn uds_suppressed_unknown_response_codes_are_preserved_in_both_modes() {
    let adapter = vcan_adapter();
    let _vecu = vecu_spawn_with_case(&adapter, Some("reserved-nrc")).await;
    let transport = suppression_transport(&adapter);
    for strict in [true, false] {
        let uds = UDSClient::with_config(&transport, suppression_config(strict));
        assert!(matches!(
            uds.suppress_positive_responses().tester_present().await,
            Err(Error::UDSError(UDSError::NegativeResponse(
                NegativeResponseCode::NonStandard(0x15)
            )))
        ));
    }
}
