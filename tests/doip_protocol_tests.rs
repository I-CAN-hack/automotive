//! DoIP wire-level timing and cancellation regressions, with no Python dependency.
use automotive::transports::doip::{DoIP, Error as DoIPError};
use automotive::{Error, StreamExt, Transport};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

fn frame(kind: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![2, 0xfd];
    bytes.extend(kind.to_be_bytes());
    bytes.extend((payload.len() as u32).to_be_bytes());
    bytes.extend(payload);
    bytes
}

async fn read(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = [0; 8];
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(&header[..2], &[2, 0xfd]);
    let mut payload = vec![0; u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize];
    stream.read_exact(&mut payload).await.unwrap();
    payload
}

async fn accept(listener: TcpListener) -> TcpStream {
    let (mut stream, _) = listener.accept().await.unwrap();
    assert_eq!(read(&mut stream).await, [0x0e, 0, 0, 0, 0, 0, 0]);
    stream
        .write_all(&frame(6, &[0x0e, 0, 0x10, 0, 0x10, 0, 0, 0, 0]))
        .await
        .unwrap();
    stream
}

#[tokio::test]
async fn cancelled_send_finishes_ack_before_next_request_without_replaying() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (received, ready) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut stream = accept(listener).await;
        assert_eq!(read(&mut stream).await, [0x0e, 0, 0x10, 0, 0x3e, 0]);
        received.send(()).unwrap();
        released.await.unwrap();
        // The first request's ACK must not complete the second send.
        stream
            .write_all(&frame(0x8002, &[0x10, 0, 0x0e, 0, 0, 0x3e, 0]))
            .await
            .unwrap();
        assert_eq!(
            read(&mut stream).await,
            [0x0e, 0, 0x10, 0, 0x22, 0x12, 0x34]
        );
        stream
            .write_all(&frame(0x8003, &[0x10, 0, 0x0e, 0, 3, 0x22, 0x12, 0x34]))
            .await
            .unwrap();
    });
    let transport = DoIP::connect(address, 0x1000).await.unwrap();
    let first = transport.clone();
    let task = tokio::spawn(async move { first.send(&[0x3e, 0]).await });
    ready.await.unwrap();
    task.abort();
    let _ = task.await;
    release.send(()).unwrap();
    assert!(matches!(
        transport.send(&[0x22, 0x12, 0x34]).await,
        Err(Error::DoIPError(DoIPError::DiagnosticNack(3)))
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn cancelling_receive_preserves_partial_tcp_pdu_and_coalesced_frames() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release, released) = oneshot::channel();
    let server = tokio::spawn(async move {
        let mut stream = accept(listener).await;
        read(&mut stream).await;
        let response = frame(0x8001, &[0x10, 0, 0x0e, 0, 0x62, 0x12, 0x34, 0xab]);
        let mut first = frame(0x8002, &[0x10, 0, 0x0e, 0, 0]);
        first.extend(&response[..10]);
        stream.write_all(&first).await.unwrap();
        released.await.unwrap();
        stream.write_all(&response[10..]).await.unwrap();
        stream
            .write_all(&frame(0x8001, &[0x10, 0, 0x0e, 0, 0x7e, 0]))
            .await
            .unwrap();
    });
    let transport = DoIP::connect(address, 0x1000).await.unwrap();
    let mut receiver = transport.recv();
    transport.send(&[0x22, 0x12, 0x34]).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), receiver.next())
            .await
            .is_err()
    );
    release.send(()).unwrap();
    assert_eq!(
        receiver.next().await.unwrap().unwrap(),
        [0x62, 0x12, 0x34, 0xab]
    );
    assert_eq!(receiver.next().await.unwrap().unwrap(), [0x7e, 0]);
    assert!(matches!(
        receiver.next().await,
        Some(Err(Error::Disconnected))
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn malformed_control_lengths_are_rejected_from_header_without_body() {
    for (kind, size) in [(6u16, 8u32), (7, 1), (0x8001, 4), (0x8002, 4), (0x8003, 4)] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut stream = accept(listener).await;
            let mut header = vec![2, 0xfd];
            header.extend(kind.to_be_bytes());
            header.extend(size.to_be_bytes());
            stream.write_all(&header).await.unwrap();
        });
        let transport = DoIP::connect(address, 0x1000).await.unwrap();
        assert!(matches!(
            transport.recv().next().await,
            Some(Err(Error::DoIPError(DoIPError::InvalidMessage(_))))
        ));
        server.await.unwrap();
    }
}
