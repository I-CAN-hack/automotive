//! Scripted UDS PDUs exercise timing and existing response validation.

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
async fn default_p2_includes_network_margin_and_starts_after_send() {
    for (response_delay, succeeds) in [(75, true), (125, false)] {
        let mut transport = ScriptedTransport::new(vec![vec![(response_delay, vec![0x7e, 0])]]);
        transport.send_delay = Duration::from_millis(200);
        let uds = UDSClient::new(&transport);
        let start = tokio::time::Instant::now();
        let result = uds.tester_present().await;
        if succeeds {
            result.unwrap();
            assert_eq!(start.elapsed(), Duration::from_millis(275));
        } else {
            assert!(matches!(result, Err(Error::Timeout)));
            assert_eq!(start.elapsed(), Duration::from_millis(300));
        }
        assert_eq!(*transport.sent.lock().unwrap(), vec![vec![0x3e, 0]]);
    }
}

#[tokio::test(start_paused = true)]
async fn pending_responses_restart_p2_star_and_next_request_uses_p2() {
    let transport = ScriptedTransport::new(vec![
        vec![
            (20, vec![0x7f, 0x3e, 0x78]),
            (200, vec![0x7f, 0x3e, 0x78]),
            (200, vec![0x7e, 0]),
        ],
        vec![(100, vec![0x7e, 0])],
    ]);
    let config = UDSConfig::builder()
        .p2_timeout(Duration::from_millis(50))
        .p2_star_timeout(Duration::from_millis(200))
        .timing_margin(Duration::from_millis(25))
        .build();
    let uds = UDSClient::with_config(&transport, config);
    uds.tester_present().await.unwrap();
    assert!(matches!(uds.tester_present().await, Err(Error::Timeout)));
}

#[tokio::test(start_paused = true)]
async fn pending_response_uses_default_p2_star_deadline() {
    for (delay, succeeds) in [(5025, true), (5075, false)] {
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
            vec![(10, vec![0x7f, 0x3e, 0x78]), (400, vec![0x7e, 0])],
        ]);
        let config = UDSConfig::builder()
            .p2_timeout(Duration::from_millis(50))
            .p2_star_timeout(Duration::from_millis(100))
            .timing_margin(Duration::from_millis(25))
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
        assert!(matches!(uds.tester_present().await, Err(Error::Timeout)));
    }
}

#[tokio::test(start_paused = true)]
async fn session_without_timing_record_preserves_previous_timings() {
    let transport = ScriptedTransport::new(vec![
        vec![(0, vec![0x50, 3, 0, 200, 0, 30])],
        vec![(0, vec![0x50, 1])],
        vec![(100, vec![0x7e, 0])],
    ]);
    let uds = UDSClient::new(&transport);
    uds.diagnostic_session_control(3).await.unwrap();
    assert_eq!(uds.diagnostic_session_control(1).await.unwrap(), None);
    uds.tester_present().await.unwrap();
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
                        maximum: None,
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
async fn negative_responses_remain_errors_in_both_modes_without_new_validation() {
    for strict in [true, false] {
        // Existing behavior permits trailing data and does not validate the echoed SID.
        let transport = ScriptedTransport::new(vec![vec![(0, vec![0x7f, 0x10, 0x33, 0xaa])]]);
        let uds = UDSClient::with_config(&transport, UDSConfig::builder().strict(strict).build());
        assert!(matches!(
            uds.tester_present().await,
            Err(Error::UDSError(UDSError::NegativeResponse(
                NegativeResponseCode::SecurityAccessDenied
            )))
        ));
    }
}
