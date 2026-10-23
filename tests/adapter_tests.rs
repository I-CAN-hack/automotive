#![allow(dead_code, unused_imports)]
#[cfg(all(target_os = "linux", feature = "socketcan"))]
use automotive::adapters::socketcan::SocketCanConfig;
use automotive::can::AsyncCanAdapter;
use automotive::can::{CanAdapter, Frame, Identifier};
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

/// Must not be larger than the RX buffer size of the adapter, otherwise the test will fail due to dropped messages.
/// Should be large enough to trigger any potential issues with the adapter's internal buffering and handling of multiple messages.
static BULK_NUM_FRAMES_SYNC: usize = 0x100;
static BULK_NUM_FRAMES_ASYNC: usize = 0x1000;
static BULK_SYNC_TIMEOUT_MS: u64 = 5000;
static BULK_ASYNC_TIMEOUT_MS: u64 = 5000;

#[cfg(feature = "test-j2534")]
fn j2534_bitrate_config() -> automotive::can::bitrate::BitrateConfig {
    automotive::can::bitrate::BitrateBuilder::new::<automotive::adapters::j2534::J2534Adapter>()
        .bitrate(500_000)
        .build()
        .unwrap()
}

#[cfg(feature = "test-panda")]
fn panda_config() -> automotive::adapters::panda::PandaConfig {
    automotive::adapters::panda::PandaConfig::builder()
        .bitrate(|bitrate| bitrate.bitrate(500_000).sample_point(0.8))
        .build()
        .unwrap()
}

#[cfg(feature = "test-peak")]
fn peak_config() -> automotive::adapters::peak::PeakConfig {
    automotive::adapters::peak::PeakConfig::builder()
        .bitrate(|bitrate| bitrate.bitrate(500_000).sample_point(0.8))
        .tx_timeout(Duration::from_millis(BULK_ASYNC_TIMEOUT_MS))
        .build()
        .unwrap()
}

#[cfg(feature = "test-vector")]
fn vector_config() -> automotive::adapters::vector::VectorConfig {
    automotive::adapters::vector::VectorConfig::builder(0)
        .bitrate(|bitrate| {
            bitrate
                .bitrate(500_000)
                .sample_point(0.8)
                .sjw(1)
                .data_bitrate(2_000_000)
                .data_sample_point(0.8)
                .data_sjw(1)
        })
        .tx_timeout(Duration::from_millis(BULK_ASYNC_TIMEOUT_MS))
        .build()
        .unwrap()
}

fn get_test_frames(amount: usize) -> Vec<Frame> {
    let mut frames = vec![];

    // Extended ID
    frames.push(
        Frame::new(
            0,
            Identifier::extended(0x1234).unwrap(),
            &[0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA],
        )
        .unwrap(),
    );

    // Extended ID that also fits in Standard ID
    frames.push(
        Frame::new(
            0,
            Identifier::extended(0x123).unwrap(),
            &[0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA],
        )
        .unwrap(),
    );

    // Zero length data
    frames.push(Frame::new(0, 0x0.try_into().unwrap(), &[]).unwrap());

    // Add bulk
    for i in 0..amount {
        frames.push(Frame::new(0, 0x123.try_into().unwrap(), &i.to_be_bytes()).unwrap());
    }

    frames
}

/// Sends a large number of frames to a "blocking" adapter, and then reads back all sent messages.
/// This verifies the adapter doesn't drop messages and preserves order for each bus/identifier,
/// which is needed for the async adapter to work correctly.
fn bulk_send_sync<T: CanAdapter>(adapter: &mut T) {
    let frames = get_test_frames(BULK_NUM_FRAMES_SYNC);

    // The blocking sync test transmits everything before reading any echoes
    // back, so cap the number of frames to what the adapter can buffer at once.
    let limit = adapter.buffer_size().unwrap_or(frames.len());
    let frames: Vec<Frame> = frames.into_iter().take(limit).collect();

    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(BULK_SYNC_TIMEOUT_MS);
    let mut to_send: VecDeque<Frame> = frames.clone().into();
    while !to_send.is_empty() {
        assert!(
            start.elapsed() < timeout,
            "Timed out submitting frames: {} of {} still queued",
            to_send.len(),
            frames.len(),
        );
        let remaining = to_send.len();
        adapter.send(&mut to_send).unwrap();
        if to_send.len() == remaining {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let start = std::time::Instant::now();

    let mut received: Vec<Frame> = vec![];
    while received.len() < frames.len()
        && start.elapsed() < Duration::from_millis(BULK_SYNC_TIMEOUT_MS)
    {
        let rx = adapter.recv().unwrap();
        let rx: Vec<Frame> = rx.into_iter().filter(|frame| frame.loopback).collect();

        for frame in rx {
            let mut copy = frame.clone();
            copy.loopback = false;
            received.push(copy);
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(
        frames.len(),
        received.len(),
        "Missing transmit confirmations"
    );
    // Adapters may prioritize different arbitration IDs. Only frames sharing
    // a bus and identifier must retain their submission order.
    let mut expected: HashMap<_, VecDeque<_>> = HashMap::new();
    for frame in &frames {
        expected
            .entry((frame.bus, frame.id()))
            .or_default()
            .push_back(frame);
    }
    for (index, frame) in received.iter().enumerate() {
        let next = expected
            .get_mut(&(frame.bus, frame.id()))
            .and_then(VecDeque::pop_front);
        assert_eq!(
            Some(frame),
            next,
            "Unexpected transmit confirmation #{index}"
        );
    }
}

/// Sends a large number of frames to the adapter, and awaits them simultaneously.
/// This tests the functionality in [`AsyncCanAdapter`] to resolve the future when the message is ACKed.
async fn bulk_send(adapter: &AsyncCanAdapter) {
    let frames = get_test_frames(BULK_NUM_FRAMES_ASYNC);

    let confirmed = std::cell::Cell::new(0);
    let r = frames.iter().map(|frame| async {
        let result = adapter.send(frame).await;
        if result.is_ok() {
            confirmed.set(confirmed.get() + 1);
        }
        result
    });
    let results = tokio::time::timeout(
        Duration::from_millis(BULK_ASYNC_TIMEOUT_MS),
        futures::future::join_all(r),
    )
    .await
    .unwrap_or_else(|_| {
        panic!(
            "Timed out waiting for transmit confirmations: {} of {} confirmed",
            confirmed.get(),
            frames.len(),
        )
    });
    for result in results {
        result.unwrap();
    }
}

#[cfg(feature = "test-j2534")]
#[tokio::test]
#[serial_test::serial]
async fn j2534_bulk_send() {
    let mut j2534 =
        automotive::adapters::j2534::J2534Adapter::new(None, j2534_bitrate_config()).unwrap();
    bulk_send_sync(&mut j2534);
}

#[cfg(feature = "test-j2534")]
#[tokio::test]
#[serial_test::serial]
async fn j2534_bulk_send_async() {
    let j2534 =
        automotive::adapters::j2534::J2534Adapter::new_async(None, j2534_bitrate_config()).unwrap();
    bulk_send(&j2534).await;
}

#[cfg(feature = "test-panda")]
#[test]
#[serial_test::serial]
fn panda_bulk_send_sync() {
    let mut panda = automotive::adapters::panda::PandaAdapter::new(panda_config()).unwrap();
    bulk_send_sync(&mut panda);
}

#[cfg(feature = "test-panda")]
#[tokio::test]
#[serial_test::serial]
async fn panda_bulk_send_async() {
    let panda = automotive::adapters::panda::PandaAdapter::new_async(panda_config()).unwrap();
    bulk_send(&panda).await;
}

#[cfg(feature = "test-vector")]
#[test]
#[serial_test::serial]
fn vector_bulk_send_sync() {
    let mut vector = automotive::adapters::vector::VectorAdapter::new(vector_config()).unwrap();
    bulk_send_sync(&mut vector);
}

#[cfg(feature = "test-vector")]
#[tokio::test]
#[serial_test::serial]
async fn vector_bulk_send_async() {
    let vector = automotive::adapters::vector::VectorAdapter::new_async(vector_config()).unwrap();
    bulk_send(&vector).await;
}

#[cfg(feature = "test-peak")]
#[test]
#[serial_test::serial]
fn peak_bulk_send_sync() {
    let mut peak = automotive::adapters::peak::PeakAdapter::new(peak_config()).unwrap();
    bulk_send_sync(&mut peak);
}

#[cfg(feature = "test-peak")]
#[tokio::test]
#[serial_test::serial]
async fn peak_bulk_send_async() {
    let peak = automotive::adapters::peak::PeakAdapter::new_async(peak_config()).unwrap();
    bulk_send(&peak).await;
}

#[cfg(feature = "test-socketcan")]
#[test]
#[serial_test::serial]
fn socketcan_bulk_send_sync() {
    let mut adapter =
        automotive::adapters::socketcan::SocketCanAdapter::new(SocketCanConfig::new("can0"))
            .unwrap();
    bulk_send_sync(&mut adapter);
}

#[cfg(feature = "test-socketcan")]
#[tokio::test]
#[serial_test::serial]
async fn socketcan_bulk_send_async() {
    let adapter =
        automotive::adapters::socketcan::SocketCanAdapter::new_async(SocketCanConfig::new("can0"))
            .unwrap();
    bulk_send(&adapter).await;
}

#[cfg(feature = "test-vcan")]
#[test]
#[serial_test::serial]
fn vcan_bulk_send_sync() {
    let mut adapter =
        automotive::adapters::socketcan::SocketCanAdapter::new(SocketCanConfig::new("vcan0"))
            .unwrap();
    bulk_send_sync(&mut adapter);
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn vcan_bulk_send_async() {
    let adapter =
        automotive::adapters::socketcan::SocketCanAdapter::new_async(SocketCanConfig::new("vcan0"))
            .unwrap();
    bulk_send(&adapter).await;
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn vcan_send_fd() {
    let adapter =
        automotive::adapters::socketcan::SocketCanAdapter::new_async(SocketCanConfig::new("vcan0"))
            .unwrap();
    adapter
        .send(&Frame::new(0, 0x123.try_into().unwrap(), &[0u8; 64]).unwrap())
        .await
        .unwrap();
}

#[cfg(all(target_os = "linux", feature = "socketcan"))]
#[tokio::test]
#[serial_test::serial]
async fn socketcan_open_nonexistent() {
    let e = automotive::adapters::socketcan::SocketCanAdapter::new(SocketCanConfig::new(
        "doestnotexist",
    ));

    match e {
        Err(automotive::Error::NotFound) => {}
        _ => panic!("Expected NotFound error"),
    }
}
