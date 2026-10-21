#![cfg(all(target_os = "linux", feature = "test-vcan"))]

use automotive::adapters::socketcan::SocketCanConfig;
use automotive::can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::protocols::uds::Error as UDSError;
use automotive::protocols::uds::NegativeResponseCode;
use automotive::protocols::uds::{DTCFormatIdentifier, RoutineControlType, UDSClient, UDSConfig};
use automotive::transports::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::Error;
use automotive::StreamExt;
use std::process::{Child, Command};
use std::time::Duration;

static VECU_STARTUP_TIMEOUT_MS: u64 = 10000;

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn vecu_spawn(adapter: &AsyncCanAdapter) -> ChildGuard {
    let stream = adapter
        .recv()
        .timeout(std::time::Duration::from_millis(VECU_STARTUP_TIMEOUT_MS));
    tokio::pin!(stream);

    let vecu = ChildGuard(Command::new("scripts/vecu_uds.py").spawn().unwrap());
    stream.next().await.unwrap().expect("vecu did not start");

    vecu
}

#[tokio::test]
#[serial_test::serial]
async fn uds_test_sids() {
    let adapter =
        automotive::adapters::socketcan::SocketCan::new_async(SocketCanConfig::new("vcan0"))
            .unwrap();
    let _vecu = vecu_spawn(&adapter).await;

    let isotp_config = IsoTPConfig::builder(Identifier::standard(0x7a1).unwrap())
        .timeout(std::time::Duration::from_millis(1000))
        .build()
        .unwrap();

    let isotp = IsoTPAdapter::new(&adapter, isotp_config);
    let uds_config = UDSConfig::builder()
        .p2_timeout(Duration::from_secs(1))
        .build();
    let uds = UDSClient::with_config(&isotp, uds_config);

    uds.tester_present().await.unwrap();

    let resp = uds.diagnostic_session_control(0x2).await;
    let security_access_denied =
        UDSError::NegativeResponse(NegativeResponseCode::SecurityAccessDenied);
    assert!(matches!(resp, Err(Error::UDSError(e)) if e == security_access_denied));

    let timing = uds.diagnostic_session_control(3).await.unwrap().unwrap();
    assert_eq!(timing.p2_server_max, Duration::from_secs(1));
    assert_eq!(timing.p2_star_server_max, Duration::from_secs(5));

    assert_eq!(uds.ecu_reset(1).await.unwrap(), None);
    assert_eq!(uds.ecu_reset(4).await.unwrap(), Some(2));

    assert_eq!(uds.security_access(1, None).await.unwrap(), b"seed");
    let wrong_key = uds.security_access(2, Some(b"bad!")).await;
    assert!(matches!(
        wrong_key,
        Err(Error::UDSError(UDSError::NegativeResponse(
            NegativeResponseCode::InvalidKey
        )))
    ));
    assert!(uds
        .security_access(2, Some(b"key!"))
        .await
        .unwrap()
        .is_empty());

    assert_eq!(
        uds.read_data_by_identifier(0x1234).await.unwrap(),
        b"deadbeef"
    );
    uds.write_data_by_identifier(0x1234, b"updated!")
        .await
        .unwrap();
    assert_eq!(
        uds.read_data_by_identifier(0x1234).await.unwrap(),
        b"updated!"
    );

    let dtcs = uds
        .read_dtc_information_number_of_dtc_by_status_mask(0xff)
        .await
        .unwrap();
    assert_eq!(dtcs.dtc_status_availability_mask, 0xff);
    assert_eq!(
        dtcs.dtc_format_identifier,
        DTCFormatIdentifier::ISO_14229_1_DTCFormat
    );
    assert_eq!(dtcs.dtc_count, 2);

    let address = [0x12, 0x34, 0x56];
    let size = [0, 8];
    assert_eq!(
        uds.read_memory_by_address(&address, &size).await.unwrap(),
        b"abcdefgh"
    );
    uds.write_memory_by_address(&address, &size, b"written!")
        .await
        .unwrap();
    assert_eq!(
        uds.read_memory_by_address(&address, &size).await.unwrap(),
        b"written!"
    );

    assert_eq!(
        uds.routine_control(RoutineControlType::Start, 0x1234, None)
            .await
            .unwrap(),
        Some(vec![0])
    );
    assert_eq!(
        uds.routine_control(RoutineControlType::Stop, 0x1234, None)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        uds.routine_control(RoutineControlType::RequestResults, 0x1234, None)
            .await
            .unwrap(),
        Some(b"done".to_vec())
    );

    assert_eq!(
        uds.request_download(0, 0, &address, &size).await.unwrap(),
        10
    );
    assert_eq!(uds.transfer_data(1, Some(b"download")).await.unwrap(), None);
    assert_eq!(uds.request_transfer_exit(None).await.unwrap(), None);
    assert_eq!(
        uds.read_memory_by_address(&address, &size).await.unwrap(),
        b"download"
    );

    assert_eq!(uds.request_upload(0, 0, &address, &size).await.unwrap(), 10);
    assert_eq!(
        uds.transfer_data(1, None).await.unwrap(),
        Some(b"download".to_vec())
    );
    assert_eq!(
        uds.request_transfer_exit(Some(&[0xaa, 0x55]))
            .await
            .unwrap(),
        Some(vec![0xaa, 0x55])
    );
}
