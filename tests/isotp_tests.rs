#![allow(dead_code, unused_imports)]
use automotive::async_can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use tokio_stream::StreamExt;
use std::process::{Command, Child};
use std::vec;

static VECU_STARTUP_TIMEOUT_MS: u64 = 1000;

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.0.kill().unwrap()
    }
}

async fn vecu_spawn(adapter: &AsyncCanAdapter) -> ChildGuard {
    let stream = adapter.recv().timeout(std::time::Duration::from_millis(VECU_STARTUP_TIMEOUT_MS));
    tokio::pin!(stream);

    let vecu = ChildGuard(Command::new("scripts/vecu.py").spawn().unwrap());
    stream.next().await.unwrap().expect("vecu did not start");

    vecu
}


async fn isotp_echo(msg_len: usize) {
    let adapter = automotive::socketcan::SocketCan::new_async_from_name("vcan0").unwrap();
    let _vecu = vecu_spawn(&adapter).await;

    let config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    let isotp = IsoTPAdapter::new(&adapter, config);

    let mut stream = isotp.recv();
    let request = vec![0xaa; msg_len];
    isotp.send(&request).await.unwrap();
    let response = stream.next().await.unwrap().unwrap();

    assert_eq!(response, request);
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_single_frame() {
    isotp_echo(7).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_flow_control() {
    isotp_echo(64).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_cf_idx_overflow() {
    isotp_echo(256).await;
}
