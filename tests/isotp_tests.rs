#![allow(dead_code, unused_imports)]
use automotive::async_can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use std::process::{Child, Command};
use tokio_stream::StreamExt;

static VECU_STARTUP_TIMEOUT_MS: u64 = 10000;

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.0.kill().unwrap()
    }
}

#[derive(Default, Copy, Clone, Debug)]
struct VECUConfig {
    pub stmin: u32,
    pub bs: u32,
    pub padding: Option<u8>,
    pub fd: bool,
    pub ext_address: Option<u8>,
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

        if let Some(ext_address) = self.ext_address {
            result.push("--ext-address".to_owned());
            result.push(format!("{}", ext_address));
        }

        if self.fd {
            result.push("--fd".to_owned());
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

    let mut isotp_config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    isotp_config.padding = config.padding;
    isotp_config.fd = config.fd;
    isotp_config.ext_address = config.ext_address;
    isotp_config.timeout = std::time::Duration::from_millis(1000);

    let isotp = IsoTPAdapter::new(&adapter, isotp_config);

    let mut stream = isotp.recv();
    let request = vec![0xaa; msg_len];
    isotp.send(&request).await.unwrap();
    let response = stream.next().await.unwrap().unwrap();

    assert_eq!(response.len(), request.len());
    assert_eq!(response, request);
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_flow_control() {
    let config = VECUConfig::default();
    // Single frame
    isotp_test_echo(1, config).await;
    isotp_test_echo(7, config).await;
    // Flow control
    isotp_test_echo(62, config).await; // No padding on last CF
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
        ..Default::default()
    };

    isotp_test_echo(1, config).await;
    isotp_test_echo(5, config).await;
    isotp_test_echo(62, config).await; // No padding on last CF
    isotp_test_echo(64, config).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_stmin() {
    let stmin = std::time::Duration::from_millis(50);
    let config = VECUConfig {
        stmin: stmin.as_millis() as u32,
        ..Default::default()
    };

    let start = std::time::Instant::now();
    isotp_test_echo(64, config).await;
    assert!(start.elapsed() > stmin * 8);
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_bs() {
    for bs in 1..=8 {
        let config = VECUConfig {
            bs,
            ..Default::default()
        };
        isotp_test_echo(64, config).await;

        // TODO: can we ensure that we actually wait for the
        // flow control between blocks?
        isotp_test_echo(64, config).await;
    }
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_fd() {
    let config = VECUConfig {
        fd: true,
        ..Default::default()
    };

    // Single frame escape
    isotp_test_echo(62, config).await;

    // Single frame with some padding to reach next DLC
    isotp_test_echo(50, config).await;

    // Multiple frames
    isotp_test_echo(256, config).await;

    // First frame escape
    isotp_test_echo(5000, config).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_extended() {
    let config = VECUConfig {
        ext_address: Some(0xff),
        ..Default::default()
    };
    // Single frame
    isotp_test_echo(1, config).await;
    isotp_test_echo(7, config).await;
    // Flow control
    isotp_test_echo(62, config).await; // No padding on last CF
    isotp_test_echo(64, config).await;
    // Overflow IDX in flow control
    isotp_test_echo(256, config).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_fd_extended() {
    let config = VECUConfig {
        fd: true,
        ext_address: Some(0xff),
        ..Default::default()
    };

    // Single frame escape
    isotp_test_echo(62, config).await;

    // Single frame with some padding to reach next DLC
    isotp_test_echo(50, config).await;

    // Multiple frames
    isotp_test_echo(256, config).await;

    // First frame escape
    isotp_test_echo(5000, config).await;
}
