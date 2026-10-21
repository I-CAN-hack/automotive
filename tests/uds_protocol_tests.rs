//! Scripted UDS PDUs exercise response timing and validation.

use automotive::protocols::uds::{
    Error as UDSError, NegativeResponseCode, ProtocolDeviation, RoutineControlType, UDSClient,
    UDSConfig,
};
use automotive::{Error, Result, TransportLayer};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

type Replies = Vec<(u64, Vec<u8>)>;

struct ScriptedTransport {
    exchanges: Mutex<VecDeque<Replies>>,
    sent: Mutex<Vec<Vec<u8>>>,
    send_delay: Duration,
}

impl ScriptedTransport {
    fn new(exchanges: Vec<Replies>) -> Self {
        Self {
            exchanges: Mutex::new(exchanges.into()),
            sent: Mutex::new(Vec::new()),
            send_delay: Duration::ZERO,
        }
    }
}

impl TransportLayer for ScriptedTransport {
    async fn send(&self, data: &[u8]) -> Result<()> {
        self.sent.lock().unwrap().push(data.to_vec());
        tokio::time::sleep(self.send_delay).await;
        Ok(())
    }

    fn recv(&self) -> impl automotive::Stream<Item = Result<Vec<u8>>> + Unpin + '_ {
        let replies = self.exchanges.lock().unwrap().pop_front().unwrap();
        Box::pin(async_stream::stream! {
            for (delay_ms, payload) in replies {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                yield Ok(payload);
            }
            std::future::pending::<()>().await;
        })
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

#[tokio::test(start_paused = true)]
async fn default_p2_deadline_starts_after_send() {
    for (response_delay, succeeds) in [(25, true), (75, false)] {
        let mut transport = ScriptedTransport::new(vec![vec![(response_delay, vec![0x7e, 0])]]);
        transport.send_delay = Duration::from_millis(200);
        let uds = UDSClient::new(&transport);
        let start = tokio::time::Instant::now();
        let result = uds.tester_present().await;
        if succeeds {
            result.unwrap();
            assert_eq!(start.elapsed(), Duration::from_millis(225));
        } else {
            assert!(matches!(result, Err(Error::Timeout)));
            assert_eq!(start.elapsed(), Duration::from_millis(250));
        }
        assert_eq!(*transport.sent.lock().unwrap(), vec![vec![0x3e, 0]]);
    }
}

#[tokio::test(start_paused = true)]
async fn pending_responses_restart_p2_star_and_next_request_uses_p2() {
    let transport = ScriptedTransport::new(vec![
        vec![
            (20, vec![0x7f, 0x3e, 0x78]),
            (175, vec![0x7f, 0x3e, 0x78]),
            (175, vec![0x7e, 0]),
        ],
        vec![(100, vec![0x7e, 0])],
    ]);
    let config = UDSConfig::builder()
        .p2_timeout(Duration::from_millis(50))
        .p2_star_timeout(Duration::from_millis(200))
        .build();
    let uds = UDSClient::with_config(&transport, config);
    uds.tester_present().await.unwrap();
    assert!(matches!(uds.tester_present().await, Err(Error::Timeout)));
}

#[tokio::test(start_paused = true)]
async fn pending_response_uses_default_p2_star_deadline() {
    for (delay, succeeds) in [(4975, true), (5025, false)] {
        let transport = ScriptedTransport::new(vec![vec![
            (10, vec![0x7f, 0x3e, 0x78]),
            (delay, vec![0x7e, 0]),
        ]]);
        let uds = UDSClient::new(&transport);
        let result = uds.tester_present().await;
        if succeeds {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Timeout)));
        }
    }
}

#[tokio::test(start_paused = true)]
async fn overall_timeout_limits_pending_responses_and_sending() {
    let transport = ScriptedTransport::new(vec![vec![
        (10, vec![0x7f, 0x3e, 0x78]),
        (80, vec![0x7f, 0x3e, 0x78]),
        (80, vec![0x7e, 0]),
    ]]);
    let config = UDSConfig::builder()
        .request_timeout(Duration::from_millis(100))
        .build();
    let uds = UDSClient::with_config(&transport, config);
    let start = tokio::time::Instant::now();
    assert!(matches!(uds.tester_present().await, Err(Error::Timeout)));
    assert_eq!(start.elapsed(), Duration::from_millis(100));

    let mut transport = ScriptedTransport::new(vec![vec![(0, vec![0x7e, 0])]]);
    transport.send_delay = Duration::from_millis(200);
    let uds = UDSClient::with_config(&transport, config);
    let start = tokio::time::Instant::now();
    assert!(matches!(uds.tester_present().await, Err(Error::Timeout)));
    assert_eq!(start.elapsed(), Duration::from_millis(100));
}

#[tokio::test(start_paused = true)]
async fn session_control_decodes_and_optionally_adopts_both_server_timings() {
    for use_server_timing in [true, false] {
        let transport = ScriptedTransport::new(vec![
            vec![(0, vec![0x50, 3, 0x00, 0xc8, 0x00, 0x1e])],
            vec![(100, vec![0x7e, 0])],
            vec![(10, vec![0x7f, 0x3e, 0x78]), (200, vec![0x7e, 0])],
            vec![(10, vec![0x7f, 0x3e, 0x78]), (310, vec![0x7e, 0])],
            vec![(210, vec![0x7e, 0])],
        ]);
        let config = UDSConfig::builder()
            .p2_timeout(Duration::from_millis(50))
            .p2_star_timeout(Duration::from_millis(100))
            .use_server_timing(use_server_timing)
            .build();
        let uds = UDSClient::with_config(&transport, config);
        let timing = uds.diagnostic_session_control(3).await.unwrap().unwrap();
        assert_eq!(timing.p2_server_max, Duration::from_millis(200));
        assert_eq!(timing.p2_star_server_max, Duration::from_millis(300));
        for _ in 0..2 {
            let result = uds.tester_present().await;
            if use_server_timing {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Timeout)));
            }
        }
        let start = tokio::time::Instant::now();
        assert!(matches!(uds.tester_present().await, Err(Error::Timeout)));
        let p2_star = if use_server_timing { 300 } else { 100 };
        assert_eq!(start.elapsed(), Duration::from_millis(10 + p2_star));

        let start = tokio::time::Instant::now();
        assert!(matches!(uds.tester_present().await, Err(Error::Timeout)));
        let p2 = if use_server_timing { 200 } else { 50 };
        assert_eq!(start.elapsed(), Duration::from_millis(p2));
    }
}

#[tokio::test(start_paused = true)]
async fn session_without_timing_record_preserves_previous_timings() {
    for strict in [true, false] {
        let transport = ScriptedTransport::new(vec![
            vec![(0, vec![0x50, 3, 0, 200, 0, 30])],
            vec![(0, vec![0x50, 1])],
            vec![(100, vec![0x7e, 0])],
        ]);
        let uds = UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
        uds.diagnostic_session_control(3).await.unwrap();
        let result = uds.diagnostic_session_control(1).await;
        if strict {
            assert_deviation(
                result.unwrap_err(),
                ProtocolDeviation::InvalidResponseLength {
                    minimum: 6,
                    maximum: Some(6),
                },
                &[0x50, 1],
            );
        } else {
            assert_eq!(result.unwrap(), None);
        }
        uds.tester_present().await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn strict_service_and_sub_function_deviations_preserve_full_payload() {
    for (payload, expected) in [
        (
            vec![0x51, 0, 0xaa],
            ProtocolDeviation::InvalidServiceId {
                expected: 0x7e,
                actual: 0x51,
            },
        ),
        (
            vec![0x7e, 1, 0xbb],
            ProtocolDeviation::InvalidSubFunction {
                expected: 0,
                actual: 1,
            },
        ),
    ] {
        for strict in [true, false] {
            let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
            let config = UDSConfig::builder().strict(strict).build();
            let uds = if strict {
                UDSClient::new(&transport)
            } else {
                UDSClient::with_config(&transport, config)
            };
            let result = uds.request(0x3e, Some(0), None).await;
            if strict {
                assert_deviation(result.unwrap_err(), expected.clone(), &payload);
            } else {
                assert_eq!(result.unwrap(), payload[2..]);
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn strict_service_specific_deviations_preserve_headers_and_data() {
    for strict in [true, false] {
        let payloads = [
            vec![0x62, 0x12, 0x35, 0xaa],
            vec![0x6e, 0x12, 0x35],
            vec![0x71, 1, 0x12, 0x35, 0xbb],
            vec![0x76, 2, 0xcc],
        ];
        let transport =
            ScriptedTransport::new(payloads.iter().map(|p| vec![(0, p.clone())]).collect());
        let uds = UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
        let results = [
            uds.read_data_by_identifier(0x1234).await.map(|_| ()),
            uds.write_data_by_identifier(0x1234, &[0xaa]).await,
            uds.routine_control(RoutineControlType::Start, 0x1234, None)
                .await
                .map(|_| ()),
            uds.transfer_data(1, None).await.map(|_| ()),
        ];
        let deviations = [
            ProtocolDeviation::InvalidDataIdentifier {
                expected: 0x1234,
                actual: 0x1235,
            },
            ProtocolDeviation::InvalidDataIdentifier {
                expected: 0x1234,
                actual: 0x1235,
            },
            ProtocolDeviation::InvalidRoutineIdentifier {
                expected: 0x1234,
                actual: 0x1235,
            },
            ProtocolDeviation::InvalidBlockSequenceCounter {
                expected: 1,
                actual: 2,
            },
        ];
        for ((result, deviation), payload) in results.into_iter().zip(deviations).zip(payloads) {
            if strict {
                assert_deviation(result.unwrap_err(), deviation, &payload);
            } else {
                result.unwrap();
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn strict_format_checks_can_be_disabled() {
    for strict in [true, false] {
        let dtc = vec![0x59, 1, 0xff, 1, 0, 2, 0xaa];
        let download = vec![0x74, 0x10, 0x40, 0xbb];
        let transport =
            ScriptedTransport::new(vec![vec![(0, dtc.clone())], vec![(0, download.clone())]]);
        let uds = UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
        let count = uds
            .read_dtc_information_number_of_dtc_by_status_mask(0xff)
            .await;
        let length = uds.request_download(0, 0, &[0], &[1]).await;
        if strict {
            assert_deviation(
                count.unwrap_err(),
                ProtocolDeviation::InvalidResponseLength {
                    minimum: 6,
                    maximum: Some(6),
                },
                &dtc,
            );
            assert_deviation(
                length.unwrap_err(),
                ProtocolDeviation::InvalidResponseLength {
                    minimum: 3,
                    maximum: Some(3),
                },
                &download,
            );
        } else {
            assert_eq!(count.unwrap().dtc_count, 2);
            assert_eq!(length.unwrap(), 0x40);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn truncated_responses_are_safe_in_both_modes_and_strict_errors_include_payload() {
    for strict in [true, false] {
        for payload in [vec![], vec![0x7f], vec![0x7f, 0x3e], vec![0x7e]] {
            let minimum = if payload.first() == Some(&0x7f) {
                3
            } else if payload.is_empty() {
                1
            } else {
                2
            };
            let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
            let uds =
                UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
            let error = uds.tester_present().await.unwrap_err();
            if strict {
                assert_deviation(
                    error,
                    ProtocolDeviation::InvalidResponseLength {
                        minimum,
                        maximum: if payload.first() == Some(&0x7f) {
                            Some(3)
                        } else {
                            None
                        },
                    },
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

#[tokio::test(start_paused = true)]
async fn negative_responses_report_nrc_in_both_modes() {
    for strict in [true, false] {
        let transport = ScriptedTransport::new(vec![vec![(0, vec![0x7f, 0x3e, 0x33])]]);
        let uds = UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
        assert!(matches!(
            uds.tester_present().await,
            Err(Error::UDSError(UDSError::NegativeResponse(
                NegativeResponseCode::SecurityAccessDenied
            )))
        ));
    }
}

#[derive(Clone, Copy)]
enum Service {
    Session,
    Reset(u8),
    Seed,
    Key,
    TesterPresent,
    ReadDid,
    WriteDid,
    UploadData,
    DownloadData,
    Routine,
    TransferExit,
}

async fn service_response(service: Service, strict: bool, payload: &[u8]) -> Result<()> {
    let transport = ScriptedTransport::new(vec![vec![(0, payload.to_vec())]]);
    let uds = UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
    match service {
        Service::Session => uds.diagnostic_session_control(3).await.map(|_| ()),
        Service::Reset(reset) => uds.ecu_reset(reset).await.map(|_| ()),
        Service::Seed => uds.security_access(1, None).await.map(|_| ()),
        Service::Key => uds.security_access(2, Some(&[0xaa])).await.map(|_| ()),
        Service::TesterPresent => uds.tester_present().await,
        Service::ReadDid => uds.read_data_by_identifier(0x1234).await.map(|_| ()),
        Service::WriteDid => uds.write_data_by_identifier(0x1234, &[0xaa]).await,
        Service::UploadData => uds.transfer_data(1, None).await.map(|_| ()),
        Service::DownloadData => uds.transfer_data(1, Some(&[0xaa])).await.map(|_| ()),
        Service::Routine => uds
            .routine_control(RoutineControlType::Start, 0x1234, None)
            .await
            .map(|_| ()),
        Service::TransferExit => uds.request_transfer_exit(None).await.map(|_| ()),
    }
}

#[tokio::test(start_paused = true)]
async fn strict_service_response_lengths_match_iso_14229_1_2013() {
    for (service, payload, minimum, maximum) in [
        (Service::Session, vec![0x50, 3], 6, Some(6)),
        (Service::Session, vec![0x50, 3, 0, 50, 1], 6, Some(6)),
        (
            Service::Session,
            vec![0x50, 3, 0, 50, 1, 0xf4, 0],
            6,
            Some(6),
        ),
        (Service::Reset(4), vec![0x51, 4], 3, Some(3)),
        (Service::Reset(4), vec![0x51, 4, 0, 0], 3, Some(3)),
        (Service::Reset(1), vec![0x51, 1, 0], 2, Some(2)),
        (Service::Seed, vec![0x67, 1], 3, None),
        (Service::Key, vec![0x67, 2, 0], 2, Some(2)),
        (Service::TesterPresent, vec![0x7e, 0, 0], 2, Some(2)),
        (Service::ReadDid, vec![0x62, 0x12, 0x34], 4, None),
        (Service::WriteDid, vec![0x6e, 0x12, 0x34, 0], 3, Some(3)),
        (Service::UploadData, vec![0x76, 1], 3, None),
    ] {
        let error = service_response(service, true, &payload).await.unwrap_err();
        assert_deviation(
            error,
            ProtocolDeviation::InvalidResponseLength { minimum, maximum },
            &payload,
        );
        service_response(service, false, &payload).await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn strict_mode_accepts_mandatory_and_vehicle_specific_response_records() {
    for (service, payload) in [
        (Service::Session, vec![0x50, 3, 0, 50, 1, 0xf4]),
        (Service::Reset(1), vec![0x51, 1]),
        (Service::Reset(4), vec![0x51, 4, 0]),
        (Service::Reset(4), vec![0x51, 4, 0xff]),
        (Service::Seed, vec![0x67, 1, 0]),
        (Service::Seed, vec![0x67, 1, 0, 0, 0, 0]),
        (Service::Key, vec![0x67, 2]),
        (Service::TesterPresent, vec![0x7e, 0]),
        (Service::ReadDid, vec![0x62, 0x12, 0x34, 0]),
        (Service::WriteDid, vec![0x6e, 0x12, 0x34]),
        (Service::UploadData, vec![0x76, 1, 0]),
        (Service::DownloadData, vec![0x76, 1]),
        (Service::DownloadData, vec![0x76, 1, 0xaa, 0xbb]),
        (Service::Routine, vec![0x71, 1, 0x12, 0x34]),
        (Service::Routine, vec![0x71, 1, 0x12, 0x34, 0xaa]),
        (Service::TransferExit, vec![0x77]),
        (Service::TransferExit, vec![0x77, 0xaa]),
    ] {
        service_response(service, true, &payload).await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn negative_responses_validate_length_and_request_sid_before_processing_nrc() {
    for (payload, deviation) in [
        (
            vec![0x7f, 0x31, 0x33],
            ProtocolDeviation::InvalidRequestServiceId {
                expected: 0x22,
                actual: 0x31,
            },
        ),
        (
            vec![0x7f, 0x31, 0x78],
            ProtocolDeviation::InvalidRequestServiceId {
                expected: 0x22,
                actual: 0x31,
            },
        ),
        (
            vec![0x7f, 0x22, 0x33, 0xaa],
            ProtocolDeviation::InvalidResponseLength {
                minimum: 3,
                maximum: Some(3),
            },
        ),
        (
            vec![0x7f, 0x22, 0x78, 0xaa],
            ProtocolDeviation::InvalidResponseLength {
                minimum: 3,
                maximum: Some(3),
            },
        ),
    ] {
        for strict in [true, false] {
            let transport = ScriptedTransport::new(vec![vec![
                (0, payload.clone()),
                (100, vec![0x62, 0x12, 0x34, 0xaa]),
            ]]);
            let uds =
                UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
            let start = tokio::time::Instant::now();
            let result = uds.read_data_by_identifier(0x1234).await;
            if strict {
                assert_deviation(result.unwrap_err(), deviation.clone(), &payload);
                assert_eq!(start.elapsed(), Duration::ZERO);
            } else if payload[2] == 0x78 {
                assert_eq!(result.unwrap(), vec![0xaa]);
                assert_eq!(start.elapsed(), Duration::from_millis(100));
            } else {
                assert!(matches!(
                    result,
                    Err(Error::UDSError(UDSError::NegativeResponse(
                        NegativeResponseCode::SecurityAccessDenied
                    )))
                ));
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn strict_negative_response_codes_follow_annex_a() {
    for code in [
        0x00, 0x0f, 0x15, 0x20, 0x23, 0x30, 0x32, 0x34, 0x50, 0x6f, 0x74, 0x77, 0x80, 0x8e, 0x94,
        0xef, 0xff,
    ] {
        for strict in [true, false] {
            let payload = vec![0x7f, 0x3e, code];
            let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
            let uds =
                UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
            let error = uds.tester_present().await.unwrap_err();
            if strict {
                assert_deviation(
                    error,
                    ProtocolDeviation::InvalidNegativeResponseCode(code),
                    &payload,
                );
            } else {
                assert!(
                    matches!(error, Error::UDSError(UDSError::NegativeResponse(actual)) if actual == code.into())
                );
            }
        }
    }
    for code in [
        0x10, 0x22, 0x26, 0x38, 0x4f, 0x73, 0x7f, 0x81, 0x8d, 0x8f, 0x93, 0xf0, 0xfe,
    ] {
        let transport = ScriptedTransport::new(vec![vec![(0, vec![0x7f, 0x3e, code])]]);
        let uds = UDSClient::new(&transport);
        assert!(
            matches!(uds.tester_present().await, Err(Error::UDSError(UDSError::NegativeResponse(actual))) if actual == code.into())
        );
    }
}

#[tokio::test(start_paused = true)]
async fn positive_responses_echo_sub_function_without_the_suppression_bit() {
    let payload = vec![0x50, 3, 0, 50, 1, 0xf4];
    let transport = ScriptedTransport::new(vec![vec![(0, vec![0x7f, 0x10, 0x78]), (100, payload)]]);
    let uds = UDSClient::new(&transport);
    let timing = uds.diagnostic_session_control(0x83).await.unwrap().unwrap();
    assert_eq!(timing.p2_server_max, Duration::from_millis(50));
    assert_eq!(timing.p2_star_server_max, Duration::from_secs(5));
    assert_eq!(*transport.sent.lock().unwrap(), vec![vec![0x10, 0x83]]);

    let payload = vec![0x50, 0x83, 0, 50, 1, 0xf4];
    let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
    let uds = UDSClient::new(&transport);
    assert_deviation(
        uds.diagnostic_session_control(0x83).await.unwrap_err(),
        ProtocolDeviation::InvalidSubFunction {
            expected: 3,
            actual: 0x83,
        },
        &payload,
    );
}

#[tokio::test(start_paused = true)]
async fn read_memory_validates_the_requested_byte_count() {
    for length in [0, 1, 3, 4, 5] {
        for strict in [true, false] {
            let mut payload = vec![0x63];
            payload.extend(vec![0xaa; length]);
            let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
            let uds =
                UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
            let result = uds.read_memory_by_address(&[0x12, 0x34], &[0, 4]).await;
            assert_eq!(
                *transport.sent.lock().unwrap(),
                vec![vec![0x23, 0x22, 0x12, 0x34, 0, 4]]
            );
            if strict && length != 4 {
                assert_deviation(
                    result.unwrap_err(),
                    ProtocolDeviation::InvalidMemoryRecordLength {
                        expected: 4,
                        actual: length,
                    },
                    &payload,
                );
            } else {
                assert_eq!(result.unwrap(), vec![0xaa; length]);
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn read_memory_sizes_support_all_alfid_bytes_without_truncation() {
    let mut size = [0; 15];
    size[14] = 1;
    let transport = ScriptedTransport::new(vec![vec![(0, vec![0x63, 0xaa])]]);
    let uds = UDSClient::new(&transport);
    assert_eq!(
        uds.read_memory_by_address(&[0], &size).await.unwrap(),
        vec![0xaa]
    );

    size[0] = 1;
    let payload = vec![0x63, 0xaa];
    let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
    let uds = UDSClient::new(&transport);
    assert_deviation(
        uds.read_memory_by_address(&[0], &size).await.unwrap_err(),
        ProtocolDeviation::InvalidMemoryRecordLength {
            expected: (1u128 << 112) | 1,
            actual: 1,
        },
        &payload,
    );
}

#[tokio::test(start_paused = true)]
async fn write_memory_requires_exact_alfid_address_and_size_echoes() {
    let echo = vec![0x7d, 0x23, 0x12, 0x34, 0x56, 0, 4];
    let mut bad_format = echo.clone();
    bad_format[1] = 0x32;
    let mut bad_address = echo.clone();
    bad_address[3] = 0x35;
    let mut bad_size = echo.clone();
    bad_size[6] = 5;
    let mut extra = echo.clone();
    extra.push(0xaa);
    for (payload, deviation) in [
        (
            vec![0x7d],
            ProtocolDeviation::InvalidResponseLength {
                minimum: 7,
                maximum: Some(7),
            },
        ),
        (
            echo[..6].to_vec(),
            ProtocolDeviation::InvalidResponseLength {
                minimum: 7,
                maximum: Some(7),
            },
        ),
        (
            extra,
            ProtocolDeviation::InvalidResponseLength {
                minimum: 7,
                maximum: Some(7),
            },
        ),
        (
            bad_format,
            ProtocolDeviation::InvalidAddressAndLengthFormatIdentifier {
                expected: 0x23,
                actual: 0x32,
            },
        ),
        (
            bad_address,
            ProtocolDeviation::InvalidMemoryAddress {
                expected: vec![0x12, 0x34, 0x56],
                actual: vec![0x12, 0x35, 0x56],
            },
        ),
        (
            bad_size,
            ProtocolDeviation::InvalidMemorySize {
                expected: vec![0, 4],
                actual: vec![0, 5],
            },
        ),
    ] {
        for strict in [true, false] {
            let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
            let uds =
                UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
            let result = uds
                .write_memory_by_address(&[0x12, 0x34, 0x56], &[0, 4], &[0xaa; 4])
                .await;
            assert_eq!(
                *transport.sent.lock().unwrap(),
                vec![vec![
                    0x3d, 0x23, 0x12, 0x34, 0x56, 0, 4, 0xaa, 0xaa, 0xaa, 0xaa
                ]]
            );
            if strict {
                assert_deviation(result.unwrap_err(), deviation.clone(), &payload);
            } else {
                result.unwrap();
            }
        }
    }
    let transport = ScriptedTransport::new(vec![vec![(0, echo)]]);
    let uds = UDSClient::new(&transport);
    uds.write_memory_by_address(&[0x12, 0x34, 0x56], &[0, 4], &[0xaa; 4])
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn upload_and_download_validate_reserved_format_bits_and_block_capacity() {
    for upload in [false, true] {
        for (format, length) in [
            (0x11, 0x40),
            (0x1f, 0x40),
            (0x10, 0),
            (0x10, 1),
            (0x10, 2),
            (0x10, 3),
        ] {
            for strict in [true, false] {
                let payload = vec![if upload { 0x75 } else { 0x74 }, format, length];
                let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
                let uds =
                    UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
                let result = if upload {
                    uds.request_upload(0, 0, &[0], &[1]).await
                } else {
                    uds.request_download(0, 0, &[0], &[1]).await
                };
                if strict && format & 0x0f != 0 {
                    assert_deviation(
                        result.unwrap_err(),
                        ProtocolDeviation::InvalidLengthFormatIdentifier(format),
                        &payload,
                    );
                } else if strict && length < 3 {
                    assert_deviation(
                        result.unwrap_err(),
                        ProtocolDeviation::InvalidMaxNumberOfBlockLength(length as u128),
                        &payload,
                    );
                } else {
                    assert_eq!(result.unwrap(), length as usize);
                }
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn block_length_encodings_support_fifteen_bytes_and_report_numeric_overflow() {
    for upload in [false, true] {
        for strict in [true, false] {
            for width in [1, 8, 9, 15] {
                let mut payload = vec![if upload { 0x75 } else { 0x74 }, (width << 4) as u8];
                payload.resize(2 + width, 0);
                payload[1 + width] = 0x40;
                let transport = ScriptedTransport::new(vec![vec![(0, payload)]]);
                let uds =
                    UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
                let length = if upload {
                    uds.request_upload(0, 0, &[0], &[1]).await
                } else {
                    uds.request_download(0, 0, &[0], &[1]).await
                };
                assert_eq!(length.unwrap(), 0x40);
            }

            let width = std::mem::size_of::<usize>() + 1;
            let mut payload = vec![if upload { 0x75 } else { 0x74 }, (width << 4) as u8, 1];
            payload.resize(2 + width, 0);
            let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
            let uds =
                UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
            let result = if upload {
                uds.request_upload(0, 0, &[0], &[1]).await
            } else {
                uds.request_download(0, 0, &[0], &[1]).await
            };
            match result.unwrap_err() {
                Error::UDSError(UDSError::BlockLengthOverflow { value, response }) => {
                    assert_eq!(value, 1u128 << usize::BITS);
                    assert_eq!(response, payload);
                }
                error => panic!("Expected block-length overflow, received {error:?}"),
            }

            let payload = vec![if upload { 0x75 } else { 0x74 }, 0];
            let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
            let uds =
                UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
            let result = if upload {
                uds.request_upload(0, 0, &[0], &[1]).await
            } else {
                uds.request_download(0, 0, &[0], &[1]).await
            };
            if strict {
                assert_deviation(
                    result.unwrap_err(),
                    ProtocolDeviation::InvalidLengthFormat(0),
                    &payload,
                );
            } else {
                assert_eq!(result.unwrap(), 0);
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn reserved_dtc_formats_return_errors_with_the_strict_response_payload() {
    for strict in [true, false] {
        let payload = vec![0x59, 1, 0xff, 0xff, 0, 2];
        let transport = ScriptedTransport::new(vec![vec![(0, payload.clone())]]);
        let uds = UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
        let error = uds
            .read_dtc_information_number_of_dtc_by_status_mask(0xff)
            .await
            .unwrap_err();
        if strict {
            assert_deviation(
                error,
                ProtocolDeviation::InvalidDTCFormatIdentifier(0xff),
                &payload,
            );
        } else {
            assert!(matches!(
                error,
                Error::UDSError(UDSError::InvalidDTCFormatIdentifier(0xff))
            ));
        }
    }
}
