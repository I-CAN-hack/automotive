#![allow(dead_code, unused_imports)]
use automotive::async_can::AsyncCanAdapter;
use automotive::isotp::IsoTPAdapter;
use automotive::uds::UDSClient;
use std::process::{Child, Command};
use tokio_stream::StreamExt;

static VECU_STARTUP_TIMEOUT_MS: u64 = 1000;

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

    let vecu = ChildGuard(
        Command::new("scripts/vecu_uds.py")
            .spawn()
            .unwrap(),
    );
    stream.next().await.unwrap().expect("vecu did not start");

    vecu
}


#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn uds_test_sids() {
    let adapter = automotive::socketcan::SocketCan::new_async_from_name("vcan0").unwrap();
    let _vecu = vecu_spawn(&adapter).await;

    let isotp = IsoTPAdapter::from_id(&adapter, 0x7a1);
    let uds = UDSClient::new(&isotp);

    uds.tester_present().await.unwrap();

    let data = uds.read_data_by_identifier(0x1234).await.unwrap();
    assert_eq!(data, b"deadbeef".to_vec());
}
