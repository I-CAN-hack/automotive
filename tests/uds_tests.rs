#![allow(dead_code, unused_imports)]
use automotive::can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::uds::Error as UDSError;
use automotive::uds::NegativeResponseCode;
use automotive::uds::UDSClient;
use automotive::StreamExt;
use std::process::{Child, Command};

static VECU_STARTUP_TIMEOUT_MS: u64 = 10000;

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.0.kill().unwrap()
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

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn uds_test_sids() {
    let adapter = automotive::socketcan::SocketCan::new_async("vcan0").unwrap();
    let _vecu = vecu_spawn(&adapter).await;

    let mut isotp_config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    isotp_config.timeout = std::time::Duration::from_millis(1000);

    let isotp = IsoTPAdapter::new(&adapter, isotp_config);
    let uds = UDSClient::new(&isotp);

    uds.tester_present().await.unwrap();

    let data = uds.read_data_by_identifier(0x1234).await.unwrap();
    assert_eq!(data, b"deadbeef".to_vec());

    let resp = uds.diagnostic_session_control(0x2).await;
    let security_access_denied = UDSError::NegativeResponse(NegativeResponseCode::SecurityAccessDenied);
    assert_eq!(resp, Err(security_access_denied.into()));
}
