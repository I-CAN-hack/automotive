//! DoIP wire-level timing and cancellation regressions, with no Python dependency.
use automotive::transports::doip::{Discovery, DiscoveryConfig, DoIP, Error as DoIPError};
use automotive::{Error, StreamExt, Transport};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::oneshot,
    time::{timeout, Instant},
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

fn announcement(vin: &[u8; 17], eid: u8, gid: &[u8; 6], status: Option<u8>) -> Vec<u8> {
    let mut payload = vin.to_vec();
    payload.extend((0x1000 + u16::from(eid)).to_be_bytes());
    payload.extend([eid; 6]);
    payload.extend(gid);
    payload.push(0);
    payload.extend(status);
    frame(4, &payload)
}

async fn identification(socket: &UdpSocket) -> SocketAddr {
    let mut bytes = [0; 64];
    let (size, peer) = timeout(Duration::from_secs(8), socket.recv_from(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&bytes[..size], &[0xff, 0, 0, 1, 0, 0, 0, 0]);
    peer
}

async fn synchronized_discovery(host: &str) {
    let socket = UdpSocket::bind(format!("{host}:0")).await.unwrap();
    let discovery = Discovery::bind(DiscoveryConfig {
        bind_address: format!("{host}:0").parse().unwrap(),
        destination: socket.local_addr().unwrap(),
        timeout: Duration::from_millis(150),
        ..Default::default()
    })
    .unwrap();
    let server = tokio::spawn(async move {
        let peer = identification(&socket).await;
        let vin = b"WVWZZZ1JZXW000001";
        let gid = [0x42; 6];
        let incomplete = announcement(vin, 1, &gid, Some(0x10));
        let announced = Instant::now();
        socket.send_to(&incomplete, peer).await.unwrap();
        let mut duplicates = tokio::time::interval(Duration::from_millis(100));
        let next_request = identification(&socket);
        tokio::pin!(next_request);
        loop {
            tokio::select! {
                next_peer = &mut next_request => {
                    assert_eq!(next_peer, peer);
                    assert!(announced.elapsed() >= Duration::from_secs(5));
                    break;
                }
                _ = duplicates.tick() => {
                    socket.send_to(&incomplete, peer).await.unwrap();
                }
            }
        }
        // Fresh responses need a collection window even after the original one expired.
        tokio::time::sleep(Duration::from_millis(50)).await;
        for eid in [1, 2] {
            socket
                .send_to(&announcement(vin, eid, &gid, Some(0)), peer)
                .await
                .unwrap();
        }
    });
    let vehicles = timeout(Duration::from_secs(8), discovery.discover())
        .await
        .unwrap()
        .unwrap();
    server.await.unwrap();
    assert_eq!(vehicles.len(), 2);
    assert!(vehicles
        .iter()
        .all(|vehicle| vehicle.vin_gid_sync_status == Some(0)));
    assert_eq!(vehicles[0].eid, [1; 6]);
    assert_eq!(vehicles[1].eid, [2; 6]);
}

#[tokio::test]
async fn discovery_rescans_after_synchronization_without_restarting_on_duplicates() {
    tokio::join!(
        synchronized_discovery("127.0.0.1"),
        synchronized_discovery("[::1]")
    );
}

#[tokio::test]
async fn discovery_waits_for_each_vehicle_with_a_valid_vin_or_gid() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let discovery = Discovery::bind(DiscoveryConfig {
        bind_address: "127.0.0.1:0".parse().unwrap(),
        destination: socket.local_addr().unwrap(),
        timeout: Duration::from_millis(150),
        ..Default::default()
    })
    .unwrap();
    let server = tokio::spawn(async move {
        let peer = identification(&socket).await;
        let vin = b"WVWZZZ1JZXW000001";
        let gid = [0x42; 6];
        socket
            .send_to(&announcement(vin, 1, &[0; 6], Some(0x10)), peer)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(350)).await;
        let second_announced = Instant::now();
        socket
            .send_to(&announcement(&[0xff; 17], 2, &gid, Some(0x10)), peer)
            .await
            .unwrap();
        assert_eq!(identification(&socket).await, peer);
        socket
            .send_to(&announcement(vin, 1, &[0; 6], Some(0)), peer)
            .await
            .unwrap();
        // The second vehicle remains incomplete after the first vehicle's timer.
        socket
            .send_to(&announcement(&[0xff; 17], 2, &gid, Some(0x10)), peer)
            .await
            .unwrap();
        assert_eq!(identification(&socket).await, peer);
        assert!(second_announced.elapsed() >= Duration::from_secs(5));
        socket
            .send_to(&announcement(&[0xff; 17], 2, &gid, Some(0)), peer)
            .await
            .unwrap();
    });
    let vehicles = timeout(Duration::from_secs(8), discovery.discover())
        .await
        .unwrap()
        .unwrap();
    server.await.unwrap();
    assert_eq!(vehicles.len(), 2);
    assert!(vehicles
        .iter()
        .all(|vehicle| vehicle.vin_gid_sync_status == Some(0)));
}

#[tokio::test]
async fn discovery_only_starts_synchronization_for_incomplete_valid_identifiers() {
    for (vin, gid, status) in [
        ([0; 17], [0; 6], Some(0x10)),
        ([0xff; 17], [0xff; 6], Some(0x10)),
        (*b"WVWZZZ1JZXW000001", [0x42; 6], Some(0)),
        (*b"WVWZZZ1JZXW000001", [0x42; 6], None),
    ] {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let discovery = Discovery::bind(DiscoveryConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            destination: socket.local_addr().unwrap(),
            timeout: Duration::from_millis(50),
            ..Default::default()
        })
        .unwrap();
        let server = tokio::spawn(async move {
            let peer = identification(&socket).await;
            socket
                .send_to(&announcement(&vin, 1, &gid, status), peer)
                .await
                .unwrap();
        });
        let vehicles = timeout(Duration::from_secs(1), discovery.discover())
            .await
            .unwrap()
            .unwrap();
        server.await.unwrap();
        assert_eq!(vehicles.len(), 1);
        assert_eq!(vehicles[0].vin_gid_sync_status, status);
    }
}
