#![allow(dead_code, unused_imports)]
use automotive::can::{AsyncCanAdapter, ExtendedId, StandardId};
use automotive::can::{CanAdapter, Frame, Id};
use automotive::panda::Panda;
use std::collections::VecDeque;
use std::time::Duration;

static BULK_NUM_FRAMES_SYNC: usize = 0x100;
static BULK_NUM_FRAMES_ASYNC: usize = 0x1000;
static BULK_SYNC_TIMEOUT_MS: u64 = 1000;
static BULK_ASYNC_TIMEOUT_MS: u64 = 5000;

fn get_test_frames(amount: usize) -> Vec<Frame> {
    let mut frames = vec![];

    // Extended ID
    frames.push(
        Frame::new(
            0,
            ExtendedId::new(0x1234).unwrap().into(),
            &[0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA],
        )
        .unwrap(),
    );

    // Extended ID that also fits in Standard ID
    frames.push(
        Frame::new(
            0,
            ExtendedId::new(0x123).unwrap().into(),
            &[0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA],
        )
        .unwrap(),
    );

    // Zero length data
    frames.push(Frame::new(0, StandardId::ZERO.into(), &[]).unwrap());

    // Add bulk
    for i in 0..amount {
        frames.push(Frame::new(0, StandardId::new(0x123).unwrap().into(), &i.to_be_bytes()).unwrap());
    }

    frames
}

/// Sends a large number of frames to a "blocking" adapter, and then reads back all sent messages.
/// This verified the adapter doesn't drop messages and reads them back in the same order as they are sent,
/// which is needed for the async adapter to work correctly.
fn bulk_send_sync<T: CanAdapter>(adapter: &mut T) {
    let frames = get_test_frames(BULK_NUM_FRAMES_SYNC);

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
    let frames = get_test_frames(BULK_NUM_FRAMES_ASYNC);

    let r = frames.iter().map(|frame| adapter.send(frame));
    tokio::time::timeout(
        Duration::from_millis(BULK_ASYNC_TIMEOUT_MS),
        futures::future::join_all(r),
    )
    .await
    .unwrap();
}

#[cfg(feature = "test-panda")]
#[test]
#[serial_test::serial]
fn panda_bulk_send_sync() {
    let mut panda = Panda::new().unwrap();
    bulk_send_sync(&mut panda);
}

#[cfg(feature = "test-panda")]
#[tokio::test]
#[serial_test::serial]
async fn panda_bulk_send_async() {
    let panda = automotive::panda::Panda::new_async().unwrap();
    bulk_send(&panda).await;
}

#[cfg(feature = "test-vector")]
#[test]
#[serial_test::serial]
fn vector_bulk_send_sync() {
    let mut vector = automotive::vector::VectorCan::new(0).unwrap();
    bulk_send_sync(&mut vector);
}

#[cfg(feature = "test-vector")]
#[tokio::test]
#[serial_test::serial]
async fn vector_bulk_send_async() {
    let vector = automotive::vector::VectorCan::new_async(0).unwrap();
    bulk_send(&vector).await;
}

#[cfg(feature = "test-socketcan")]
#[test]
#[serial_test::serial]
fn socketcan_bulk_send_sync() {
    use socketcan::Socket;
    let mut adapter = automotive::socketcan::SocketCan::new("can0").unwrap();
    bulk_send_sync(&mut adapter);
}

#[cfg(feature = "test-socketcan")]
#[tokio::test]
#[serial_test::serial]
async fn socketcan_bulk_send_async() {
    let adapter = automotive::socketcan::SocketCan::new_async("can0").unwrap();
    bulk_send(&adapter).await;
}

#[cfg(feature = "test-vcan")]
#[test]
#[serial_test::serial]
fn vcan_bulk_send_sync() {
    let mut adapter = automotive::socketcan::SocketCan::new("vcan0").unwrap();
    bulk_send_sync(&mut adapter);
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn vcan_bulk_send_async() {
    let adapter = automotive::socketcan::SocketCan::new_async("vcan0").unwrap();
    bulk_send(&adapter).await;
}

#[cfg(feature = "test-vcan")]
#[tokio::test]
#[serial_test::serial]
async fn vcan_send_fd() {
    let adapter = automotive::socketcan::SocketCan::new_async("vcan0").unwrap();
    adapter
        .send(&Frame::new(0, StandardId::new(0x123).unwrap().into(), &[0u8; 64]).unwrap())
        .await;
}

#[cfg(all(target_os = "linux", feature = "socketcan"))]
#[tokio::test]
#[serial_test::serial]
async fn socketcan_open_nonexistent() {
    let e = automotive::socketcan::SocketCan::new("doestnotexist");

    match e {
        Err(automotive::Error::NotFound) => {}
        _ => panic!("Expected NotFound error"),
    }
}
