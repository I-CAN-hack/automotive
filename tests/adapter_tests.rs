#![allow(dead_code, unused_imports)]
use automotive::can::AsyncCanAdapter;
use automotive::can::{CanAdapter, Frame};
use automotive::panda::Panda;
use std::time::Duration;

// static BULK_NUM_FRAMES: u64 = 0x400;
static BULK_NUM_FRAMES: u64 = 0x10;
static BULK_TIMEOUT_MS: u64 = 1000;

/// Sends a large number of frames to a "blocking" adapter, and then reads back all sent messages.
/// This verified the adapter doesn't drop messages and reads them back in the same order as they are sent,
/// which is needed for the async adapter to work correctly.
fn bulk_send_sync<T: CanAdapter>(adapter: &mut T) {
    let mut frames = vec![];

    for i in 0..BULK_NUM_FRAMES {
        frames.push(Frame::new(0, 0x123.into(), &i.to_be_bytes()).unwrap());
    }

    adapter.send(&frames).unwrap();

    let start = std::time::Instant::now();

    let mut received: Vec<Frame> = vec![];
    while received.len() < frames.len() && start.elapsed() < Duration::from_millis(BULK_TIMEOUT_MS) {
        let rx = adapter.recv().unwrap();
        let rx: Vec<Frame> = rx.into_iter().filter(|frame| frame.loopback).collect();

        for frame in rx {
            let mut copy = frame.clone();
            copy.loopback = false;
            received.push(copy);
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(frames, received);
}

/// Sends a large number of frames to the adapter, and awaits them simultaneously.
/// This tests the functionality in [`AsyncCanAdapter`] to resolve the future when the message is ACKed.
async fn bulk_send(adapter: &AsyncCanAdapter) {
    let mut frames = vec![];

    for i in 0..BULK_NUM_FRAMES {
        frames.push(Frame::new(0, 0x123.into(), &i.to_be_bytes()).unwrap());
    }

    let r = frames.iter().map(|frame| adapter.send(frame));
    tokio::time::timeout(Duration::from_millis(BULK_TIMEOUT_MS), futures::future::join_all(r))
        .await
        .unwrap();
}

#[cfg(feature = "test_panda")]
#[test]
#[serial_test::serial]
fn panda_bulk_send_sync() {
    let mut panda = Panda::new().unwrap();
    bulk_send_sync(&mut panda);
}

#[cfg(feature = "test_panda")]
#[tokio::test]
#[serial_test::serial]
async fn panda_bulk_send_async() {
    let panda = automotive::panda::Panda::new_async().unwrap();
    bulk_send(&panda).await;
}

#[cfg(feature = "test_socketcan")]
#[test]
#[serial_test::serial]
fn socketcan_bulk_send_sync() {
    use socketcan::Socket;
    let socket = socketcan::CanFdSocket::open("can0").unwrap();
    let mut adapter = automotive::socketcan::SocketCan::new(socket);
    bulk_send_sync(&mut adapter);
}

#[cfg(feature = "test_socketcan")]
#[tokio::test]
#[serial_test::serial]
async fn socketcan_bulk_send_async() {
    let adapter = automotive::socketcan::SocketCan::new_async_from_name("can0").unwrap();
    bulk_send(&adapter).await;
}

// #[cfg(feature = "test_socketcan")]
// #[tokio::test]
// #[serial_test::serial]
// async fn vcan_bulk_send_fd() {
//     let adapter = automotive::socketcan::SocketCan::new_async_from_name("can0").unwrap();
//     adapter.send(&Frame::new(0, 0x123.into(), &[0u8; 64])).await;
// }


#[cfg(feature = "test_vector")]
#[test]
#[serial_test::serial]
fn vector_bulk_send_sync() {
    use automotive::vector::wrapper;

    wrapper::open_driver();
    let mut vector = automotive::vector::VectorCan::default();
    std::thread::sleep(std::time::Duration::from_secs(10));
    bulk_send_sync(&mut vector);
}

#[cfg(feature = "test_vector")]
#[tokio::test]
#[serial_test::serial]
async fn vector_bulk_send_async() {
    let panda = automotive::panda::Panda::new_async().unwrap();
    bulk_send(&panda).await;
}

#[cfg(feature = "test_vcan")]
#[test]
#[serial_test::serial]
fn vcan_bulk_send_sync() {
    use socketcan::Socket;
    let socket = socketcan::CanFdSocket::open("vcan0").unwrap();
    let mut adapter = automotive::socketcan::SocketCan::new(socket);
    bulk_send_sync(&mut adapter);
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn vcan_bulk_send_async() {
    let adapter = automotive::socketcan::SocketCan::new_async_from_name("vcan0").unwrap();
    bulk_send(&adapter).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn vcan_bulk_send_fd() {
    let adapter = automotive::socketcan::SocketCan::new_async_from_name("vcan0").unwrap();
    adapter.send(&Frame::new(0, 0x123.into(), &[0u8; 64]).unwrap()).await;
}
