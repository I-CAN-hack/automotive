//! XCP wire behavior that needs malformed/late traffic beyond the Scapy ECU.

use std::{
    collections::VecDeque,
    sync::{mpsc, Arc},
    time::Duration,
};

use automotive::protocols::xcp::{XCPCanConfig, XCPOnCAN, XCPOnTCP, XCPOnUDP};
use automotive::{
    can::{AsyncCanAdapter, CanAdapter, Frame, Identifier},
    protocols::xcp::{ByteOrder, Error as XCPError, XCPClient, XCPConfig},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};

struct CanPeer {
    incoming: mpsc::Sender<Frame>,
    outgoing: UnboundedReceiver<Frame>,
}
struct ScriptedCan {
    incoming: mpsc::Receiver<Frame>,
    outgoing: UnboundedSender<Frame>,
}

impl CanAdapter for ScriptedCan {
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> automotive::Result<()> {
        for frame in frames.drain(..) {
            self.outgoing
                .send(frame)
                .map_err(|_| automotive::Error::Disconnected)?;
        }
        Ok(())
    }
    fn recv(&mut self) -> automotive::Result<Vec<Frame>> {
        Ok(self.incoming.try_iter().collect())
    }
}

impl CanPeer {
    async fn next(&mut self) -> Frame {
        let frame = tokio::time::timeout(Duration::from_secs(2), self.outgoing.recv())
            .await
            .unwrap()
            .unwrap();
        let mut echo = frame.clone();
        echo.loopback = true;
        self.incoming.send(echo).unwrap();
        frame
    }
}

fn can_client(fd: bool) -> (XCPClient, CanPeer) {
    let (incoming, rx) = mpsc::channel();
    let (tx, outgoing) = unbounded_channel();
    let adapter = Arc::new(AsyncCanAdapter::new(ScriptedCan {
        incoming: rx,
        outgoing: tx,
    }));
    let mut config = XCPCanConfig::new(
        Identifier::extended(0x6c3).unwrap(),
        Identifier::extended(0x6c4).unwrap(),
    );
    config.bus = 3;
    config.fd = fd;
    (
        XCPClient::new(XCPOnCAN::new(adapter, config).unwrap()).unwrap(),
        CanPeer { incoming, outgoing },
    )
}

fn reply(fd: bool, data: &[u8]) -> Frame {
    Frame::builder(Identifier::extended(0x6c4).unwrap())
        .bus(3)
        .fd(fd)
        .data(data)
        .build()
        .unwrap()
}

#[tokio::test]
async fn can_and_fd_padding_filtering_and_negotiated_short_upload() {
    for fd in [false, true] {
        let (client, mut peer) = can_client(fd);
        let task = tokio::spawn(async move {
            let request = peer.next().await;
            assert_eq!(request.id(), Some(Identifier::extended(0x6c3).unwrap()));
            assert_eq!(request.bus, 3);
            assert_eq!(request.is_fd(), fd);
            assert_eq!(
                matches!(
                    request.content(),
                    automotive::can::FrameContent::Fd { brs: true, .. }
                ),
                fd
            );
            assert_eq!(&request.data()[..8], &[0xff, 0, 0, 0, 0, 0, 0, 0]);
            assert_eq!(request.data().len(), if fd { 64 } else { 8 });
            let capacity = if fd { 64 } else { 8 };
            let connect = [0xff, 1, 0, capacity, capacity, 0, 1, 1];
            let mut wrong_bus = reply(fd, &connect);
            wrong_bus.bus = 2;
            peer.incoming.send(wrong_bus).unwrap();
            let wrong_format = Frame::builder(Identifier::standard(0x6c4).unwrap())
                .bus(3)
                .fd(fd)
                .data(&connect)
                .build()
                .unwrap();
            peer.incoming.send(wrong_format).unwrap();
            peer.incoming.send(reply(fd, &connect)).unwrap();
            let request = peer.next().await;
            assert_eq!(request.data().len(), capacity as usize);
            assert_eq!(
                &request.data()[..8],
                &[0xf4, capacity - 1, 0, 0x5a, 0, 0, 0, 0x80]
            );
            let mut data = vec![0xff];
            data.extend(0..capacity - 1);
            peer.incoming.send(reply(fd, &data)).unwrap();
            let request = peer.next().await;
            assert_eq!(request.data()[0], 0xfe);
            peer.incoming.send(reply(fd, &[0xff])).unwrap();
        });
        let info = client.connect(0).await.unwrap();
        let bytes = client
            .short_upload(0x8000_0000, info.max_upload_size(), 0x5a)
            .await
            .unwrap();
        assert_eq!(bytes, (0..if fd { 63 } else { 7 }).collect::<Vec<_>>());
        client.disconnect().await.unwrap();
        client.close().await;
        task.await.unwrap();
    }
}

async fn read_request(stream: &mut TcpStream, expected_counter: u16) -> Vec<u8> {
    let mut header = [0; 4];
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(u16::from_le_bytes([header[2], header[3]]), expected_counter);
    let length = u16::from_le_bytes([header[0], header[1]]) as usize;
    let mut data = vec![0; length];
    stream.read_exact(&mut data).await.unwrap();
    data
}

fn ethernet(counter: u16, data: &[u8]) -> Vec<u8> {
    let mut frame = (data.len() as u16).to_le_bytes().to_vec();
    frame.extend(counter.to_le_bytes());
    frame.extend(data);
    frame
}

#[tokio::test]
async fn tcp_little_endian_transport_big_endian_commands_and_counter_wrap() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        assert_eq!(read_request(&mut socket, 0).await, [0xff, 0]);
        socket
            .write_all(&ethernet(0xfffe, &[0xff, 1, 5, 12, 0, 12, 1, 1]))
            .await
            .unwrap();
        assert_eq!(
            read_request(&mut socket, 1).await,
            [0xf4, 2, 0, 0, 0x80, 0, 0, 0]
        );
        let mut packets = ethernet(0xffff, &[0xfd, 6]);
        packets.extend(ethernet(0, &[0xff, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]));
        for byte in packets {
            socket.write_all(&[byte]).await.unwrap();
            tokio::task::yield_now().await;
        }
    });
    let client = tcp_client(address, XCPConfig::default()).await.unwrap();
    let info = client.connect(0).await.unwrap();
    assert_eq!(info.byte_order, ByteOrder::BigEndian);
    assert_eq!(info.address_granularity, 4);
    assert_eq!(
        client.short_upload(0x8000_0000, 8, 0).await.unwrap(),
        [1, 2, 3, 4, 5, 6, 7, 8]
    );
    client.close().await;
    peer.await.unwrap();
}

#[tokio::test]
async fn tcp_timeout_preserves_partial_packet_until_synch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        assert_eq!(read_request(&mut socket, 0).await, [0xff, 0]);
        socket
            .write_all(&ethernet(8, &[0xff, 1, 0, 8, 8, 0, 1, 1]))
            .await
            .unwrap();
        assert_eq!(read_request(&mut socket, 1).await, [0xfd]);
        let late = ethernet(9, &[0xff, 0, 0, 0, 0x34, 0x12]);
        socket.write_all(&late[..2]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
        socket.write_all(&late[2..]).await.unwrap();
        assert_eq!(read_request(&mut socket, 2).await, [0xfc]);
        socket.write_all(&ethernet(10, &[0xfe, 0])).await.unwrap();
        assert_eq!(
            read_request(&mut socket, 3).await,
            [0xf4, 1, 0, 0, 0, 0, 0, 0x80]
        );
        socket
            .write_all(&ethernet(11, &[0xff, 0xab]))
            .await
            .unwrap();
    });
    let client = tcp_client(
        address,
        XCPConfig {
            timeout: Duration::from_millis(50),
        },
    )
    .await
    .unwrap();
    client.connect(0).await.unwrap();
    assert!(matches!(
        client.get_status().await,
        Err(automotive::Error::Timeout)
    ));
    assert!(matches!(
        client.short_upload(0, 1, 0).await,
        Err(automotive::Error::XCPError(
            XCPError::SynchronizationRequired
        ))
    ));
    client.synchronize().await.unwrap();
    assert_eq!(
        client.short_upload(0x8000_0000, 1, 0).await.unwrap(),
        [0xab]
    );
    client.close().await;
    peer.await.unwrap();
}

#[tokio::test]
async fn udp_rejects_packets_crossing_datagram_boundaries() {
    let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = server.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let mut bytes = [0; 512];
        let (_, sender) = server.recv_from(&mut bytes).await.unwrap();
        server
            .send_to(&ethernet(0, &[0xff, 1, 0, 8, 8, 0, 1, 1]), sender)
            .await
            .unwrap();
        server.recv_from(&mut bytes).await.unwrap();
        let response = ethernet(1, &[0xff, 0, 0, 0, 0, 0]);
        server.send_to(&response[..5], sender).await.unwrap();
        server.send_to(&response[5..], sender).await.unwrap();
    });
    let client = udp_client(address, XCPConfig::default()).await.unwrap();
    client.connect(0).await.unwrap();
    assert!(matches!(
        client.get_status().await,
        Err(automotive::Error::XCPTransportError(
            automotive::protocols::xcp::transport::Error::InvalidPacket { .. }
        ))
    ));
    client.close().await;
    peer.await.unwrap();
}

#[tokio::test]
async fn cancelled_queued_command_never_reaches_the_ecu() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (started, active) = tokio::sync::oneshot::channel();
    let (resume, ready) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket, 0).await;
        socket
            .write_all(&ethernet(0, &[0xff, 1, 0, 8, 8, 0, 1, 1]))
            .await
            .unwrap();
        assert_eq!(read_request(&mut socket, 1).await, [0xfd]);
        started.send(()).unwrap();
        ready.await.unwrap();
        socket
            .write_all(&ethernet(1, &[0xff, 0, 0, 0, 0, 0]))
            .await
            .unwrap();
        assert_eq!(
            read_request(&mut socket, 2).await,
            [0xf4, 1, 0, 0, 0, 0, 0, 0x80]
        );
        socket.write_all(&ethernet(2, &[0xff, 42])).await.unwrap();
    });
    let client = tcp_client(address, XCPConfig::default()).await.unwrap();
    client.connect(0).await.unwrap();
    let copy = client.clone();
    let request = tokio::spawn(async move { copy.get_status().await });
    active.await.unwrap();
    let copy = client.clone();
    let queued = tokio::spawn(async move { copy.get_status().await });
    tokio::task::yield_now().await;
    queued.abort();
    let _ = queued.await;
    request.abort();
    let _ = request.await;
    resume.send(()).unwrap();
    assert_eq!(client.short_upload(0x8000_0000, 1, 0).await.unwrap(), [42]);
    client.close().await;
    peer.await.unwrap();
}

async fn tcp_client(
    address: std::net::SocketAddr,
    config: XCPConfig,
) -> automotive::Result<XCPClient> {
    XCPClient::with_config(
        XCPOnTCP::connect_with_timeout(address, config.timeout).await?,
        config,
    )
}
async fn udp_client(
    address: std::net::SocketAddr,
    config: XCPConfig,
) -> automotive::Result<XCPClient> {
    XCPClient::with_config(
        XCPOnUDP::connect_with_timeout(address, config.timeout).await?,
        config,
    )
}
