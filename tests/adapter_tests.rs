#![allow(dead_code, unused_imports)]
use automotive::can::AsyncCanAdapter;
use automotive::can::{CanAdapter, Frame};
use automotive::panda::Panda;
use std::collections::VecDeque;
use std::time::Duration;

static BULK_NUM_FRAMES_SYNC: u64 = 0x100;
static BULK_NUM_FRAMES_ASYNC: u64 = 0x1000;
static BULK_SYNC_TIMEOUT_MS: u64 = 1000;
static BULK_ASYNC_TIMEOUT_MS: u64 = 5000;

/// Sends a large number of frames to a "blocking" adapter, and then reads back all sent messages.
/// This verified the adapter doesn't drop messages and reads them back in the same order as they are sent,
/// which is needed for the async adapter to work correctly.
fn bulk_send_sync<T: CanAdapter>(adapter: &mut T) {
    let mut frames = vec![];

    for i in 0..BULK_NUM_FRAMES_SYNC {
        frames.push(Frame::new(0, 0x123.into(), &i.to_be_bytes()).unwrap());
    }

    let mut to_send: VecDeque<Frame> = frames.clone().into();
    while !to_send.is_empty() {
        adapter.send(&mut to_send).unwrap();
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

    assert_eq!(frames.len(), received.len());
    assert_eq!(frames, received);
}

/// Sends a large number of frames to the adapter, and awaits them simultaneously.
/// This tests the functionality in [`AsyncCanAdapter`] to resolve the future when the message is ACKed.
async fn bulk_send(adapter: &AsyncCanAdapter) {
    let mut frames = vec![];

    for i in 0..BULK_NUM_FRAMES_ASYNC {
        frames.push(Frame::new(0, 0x123.into(), &i.to_be_bytes()).unwrap());
    }

    let r = frames.iter().map(|frame| adapter.send(frame));
    tokio::time::timeout(
        Duration::from_millis(BULK_ASYNC_TIMEOUT_MS),
        futures::future::join_all(r),
    )
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
    let mut adapter = automotive::socketcan::SocketCan::new("can0").unwrap();
    bulk_send_sync(&mut adapter);
}

#[cfg(feature = "test_socketcan")]
#[tokio::test]
#[serial_test::serial]
async fn socketcan_bulk_send_async() {
    let adapter = automotive::socketcan::SocketCan::new_async("can0").unwrap();
    bulk_send(&adapter).await;
}

// #[cfg(feature = "test_socketcan")]
// #[tokio::test]
// #[serial_test::serial]
// async fn vcan_bulk_send_fd() {
//     let adapter = automotive::socketcan::SocketCan::new_async("can0").unwrap();
//     adapter.send(&Frame::new(0, 0x123.into(), &[0u8; 64])).await;
// }

#[cfg(feature = "test_vcan")]
#[test]
#[serial_test::serial]
fn vcan_bulk_send_sync() {
    let mut adapter = automotive::socketcan::SocketCan::new("vcan0").unwrap();
    bulk_send_sync(&mut adapter);
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn vcan_bulk_send_async() {
    let adapter = automotive::socketcan::SocketCan::new_async("vcan0").unwrap();
    bulk_send(&adapter).await;
}

#[cfg(feature = "test_vcan")]
#[tokio::test]
#[serial_test::serial]
async fn vcan_bulk_send_fd() {
    let adapter = automotive::socketcan::SocketCan::new_async("vcan0").unwrap();
    adapter
        .send(&Frame::new(0, 0x123.into(), &[0u8; 64]).unwrap())
        .await;
}

#[tokio::test]
#[serial_test::serial]
async fn socketcan_open_nonexistent() {
    let e = automotive::socketcan::SocketCan::new("doestnotexist");

    match e {
        Err(automotive::Error::NotFound) => {}
        _ => panic!("Expected NotFound error"),
    }
}
