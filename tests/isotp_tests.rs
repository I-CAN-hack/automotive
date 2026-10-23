#![allow(dead_code, unused_imports)]
#[cfg(all(target_os = "linux", feature = "socketcan"))]
use automotive::adapters::socketcan::SocketCanConfig;
use automotive::can::AsyncCanAdapter;
use automotive::can::{Frame, Identifier};
use automotive::transports::isotp::{IsoTP, IsoTPConfig};
use automotive::StreamExt;
use std::process::{Child, Command};

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
    pub receive_bs: u8,
    pub padding: Option<u8>,
    pub fd: bool,
    pub ext_address: Option<u8>,
    pub max_dlen: Option<usize>,
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

#[cfg(feature = "test-vcan")]
async fn isotp_test_echo(msg_len: usize, config: VECUConfig) {
    let adapter =
        automotive::adapters::socketcan::SocketCanAdapter::new_async(SocketCanConfig::new("vcan0"))
            .unwrap();
    let _vecu = vecu_spawn(&adapter, config).await;

    let mut builder = IsoTPConfig::builder(Identifier::standard(0x7a1).unwrap())
        .without_padding()
        .fd(config.fd)
        .block_size(config.receive_bs)
        .timeout(std::time::Duration::from_millis(1000));
    if let Some(byte) = config.padding {
        builder = builder.padding(byte);
    }
    if let Some(address) = config.ext_address {
        builder = builder.ext_address(address);
    }
    if let Some(length) = config.max_dlen {
        builder = builder.max_dlen(length);
    }
    let isotp_config = builder.build().unwrap();

    let isotp = IsoTP::new(&adapter, isotp_config);

    let mut stream = isotp.recv();
    // A separate socket observes wire order even when TX ACKs are emulated.
    let monitor =
        automotive::adapters::socketcan::SocketCanAdapter::new_async(SocketCanConfig::new("vcan0"))
            .unwrap();
    let mut can_stream = monitor.recv();
    let marker = Frame::new(0, Identifier::standard(0x123).unwrap(), &[]).unwrap();
    let request = vec![0xcc; msg_len];
    let exchange = async {
        isotp.send(&request).await.unwrap();
        let response = stream.next().await.unwrap().unwrap();
        // End the capture after all frames from the exchange, including FCs.
        adapter.send(&marker).await.unwrap();
        response
    };
    let capture = async {
        let mut frames = Vec::new();
        loop {
            let frame = can_stream.next().await.unwrap();
            if frame.id() == marker.id() {
                break frames;
            }
            frames.push(frame);
        }
    };
    let (response, frames) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(exchange, capture)
    })
    .await
    .unwrap();

    assert_eq!(response.len(), request.len());
    assert_eq!(response, request);

    let offset = usize::from(config.ext_address.is_some());
    assert_block_size(
        &frames,
        isotp_config.tx_id,
        isotp_config.rx_id,
        config.bs.try_into().unwrap(),
        offset,
    );
    assert_block_size(
        &frames,
        isotp_config.rx_id,
        isotp_config.tx_id,
        config.receive_bs,
        offset,
    );
}

/// Check the CAN trace for one direction of the Scapy echo exchange.
fn assert_block_size(
    frames: &[Frame],
    sender: Identifier,
    receiver: Identifier,
    block_size: u8,
    offset: usize,
) {
    let mut segmented = false;
    let mut flow_controls = 0;
    let mut consecutive_frames = 0usize;
    let mut frames_in_block = 0;

    for frame in frames {
        let Some(data) = frame.data().get(offset..).filter(|data| !data.is_empty()) else {
            continue;
        };
        match (frame.id(), data[0] & 0xf0) {
            (Some(id), 0x10) if id == sender => segmented = true,
            (Some(id), 0x30) if id == receiver && segmented => {
                assert_eq!(data[0], 0x30, "expected ContinueToSend");
                assert_eq!(data[1], block_size, "wrong advertised block size");
                if flow_controls > 0 {
                    assert_ne!(block_size, 0, "unlimited blocks need only one FC");
                    assert_eq!(
                        frames_in_block,
                        usize::from(block_size),
                        "FC before block ended"
                    );
                }
                frames_in_block = 0;
                flow_controls += 1;
            }
            (Some(id), 0x20) if id == sender && segmented => {
                assert!(flow_controls > 0, "CF before initial flow control");
                if block_size != 0 {
                    assert!(
                        frames_in_block < usize::from(block_size),
                        "CF exceeded block size"
                    );
                }
                frames_in_block += 1;
                consecutive_frames += 1;
            }
            _ => {}
        }
    }

    if segmented {
        assert!(consecutive_frames > 0);
        let expected = if block_size == 0 {
            1
        } else {
            consecutive_frames.div_ceil(usize::from(block_size))
        };
        assert_eq!(
            flow_controls, expected,
            "wrong FC count, including at final block boundary"
        );
    }
}

#[cfg(feature = "test-vcan")]
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

#[cfg(feature = "test-vcan")]
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

#[cfg(feature = "test-vcan")]
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

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_bs() {
    for bs in 1..=8 {
        let config = VECUConfig {
            bs,
            ..Default::default()
        };
        isotp_test_echo(64, config).await;

        isotp_test_echo(62, config).await;
    }
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_receive_bs() {
    for receive_bs in [0, 1, 2, 8, 255] {
        let config = VECUConfig {
            receive_bs,
            ..Default::default()
        };
        // Two complete blocks, then a partial final CF requiring another FC.
        let length = 6 + 2 * usize::from(receive_bs.max(1)) * 7;
        isotp_test_echo(length, config).await;
        isotp_test_echo(length + 1, config).await;
    }
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_fd() {
    let config = VECUConfig {
        fd: true,
        ..Default::default()
    };

    // Single frame with some padding to reach next DLC
    isotp_test_echo(6, config).await;
    isotp_test_echo(7, config).await;
    isotp_test_echo(8, config).await;

    // Single frame escape
    isotp_test_echo(62, config).await;

    // Single frame with some padding to reach next DLC
    isotp_test_echo(50, config).await;

    // Multiple frames
    isotp_test_echo(218, config).await;
    isotp_test_echo(256, config).await;

    // First frame escape
    isotp_test_echo(5000, config).await;
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_fd_max_dlen() {
    let config = VECUConfig {
        fd: true,
        max_dlen: Some(8),
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

#[cfg(feature = "test-vcan")]
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
    // Max transfer size
    isotp_test_echo(4095, config).await;
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_fd_extended() {
    let config = VECUConfig {
        fd: true,
        ext_address: Some(0xff),
        ..Default::default()
    };

    // Single frame with some padding to reach next DLC
    isotp_test_echo(6, config).await;
    isotp_test_echo(7, config).await;
    isotp_test_echo(8, config).await;

    // Single frame escape
    isotp_test_echo(62, config).await;

    // Single frame with some padding to reach next DLC
    isotp_test_echo(50, config).await;

    // Multiple frames
    isotp_test_echo(256, config).await;

    // First frame escape
    isotp_test_echo(5000, config).await;
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn isotp_test_fd_extended_max_dlen() {
    let config = VECUConfig {
        fd: true,
        ext_address: Some(0xff),
        max_dlen: Some(8),
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
