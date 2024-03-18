#![allow(dead_code, unused_imports)]
use automotive::async_can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use std::process::{Child, Command};
use std::{default, vec};
use tokio_stream::StreamExt;

static VECU_STARTUP_TIMEOUT_MS: u64 = 1000;

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.0.kill().unwrap()
    }
}

#[derive(Default, Copy, Clone)]
struct VECUConfig {
    pub stmin: u32,
    pub bs: u32,
    pub padding: Option<u8>,
}

impl VECUConfig {
    fn args(&self) -> Vec<String> {
        let mut result = vec![];

        result.push("--stmin".to_owned());
        result.push(format!("{}", self.stmin));

        result.push("--bs".to_owned());
        result.push(format!("{}", self.bs));

        if let Some(padding) = self.padding {
            result.push("--padding".to_owned());
            result.push(format!("{}", padding));
        }

        result
    }
}

async fn vecu_spawn(adapter: &AsyncCanAdapter, config: VECUConfig) -> ChildGuard {
    let stream = adapter
        .recv()
        .timeout(std::time::Duration::from_millis(VECU_STARTUP_TIMEOUT_MS));
    tokio::pin!(stream);

    let vecu = ChildGuard(
        Command::new("scripts/vecu_isotp.py")
            .args(config.args())
            .spawn()
            .unwrap(),
    );
    stream.next().await.unwrap().expect("vecu did not start");

    vecu
}

async fn isotp_test_echo(msg_len: usize, config: VECUConfig) {
    let adapter = automotive::socketcan::SocketCan::new_async_from_name("vcan0").unwrap();
    let _vecu = vecu_spawn(&adapter, config).await;

    let mut config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    config.padding = config.padding;

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
async fn isotp_test_flow_control() {
    let config = VECUConfig::default();
    // Single frame
    isotp_test_echo(7, config).await;
    // Flow control
    isotp_test_echo(64, config).await;
    // Overflow IDX in flow control
    isotp_test_echo(256, config).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_padding() {
    let config = VECUConfig {
        padding: Some(0xCC),
        ..default::Default::default()
    };
    isotp_test_echo(5, config).await;
    isotp_test_echo(64, config).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_stmin() {
    let stmin = std::time::Duration::from_millis(50);
    let config = VECUConfig {
        stmin: stmin.as_millis() as u32,
        ..default::Default::default()
    };

    let start = std::time::Instant::now();
    isotp_test_echo(64, config).await;
    assert!(start.elapsed() > stmin * 8);
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_bs() {
    let config = VECUConfig {
        bs: 4,
        ..default::Default::default()
    };
    isotp_test_echo(64, config).await;
}
