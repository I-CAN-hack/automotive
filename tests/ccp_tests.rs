//! CCP 2.1 CRO/CRM wire behavior, including both ECU address byte orders.

use automotive::{
    can::{AsyncCanAdapter, CanAdapter, Frame, Identifier},
    protocols::ccp::{
        ByteOrder, CCPCanConfig, CCPClient, CCPConfig, CCPOnCAN, Command, Error, ErrorCode,
        Resource,
    },
};
use std::{
    collections::VecDeque,
    sync::{mpsc, Arc},
    time::Duration,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

struct ScriptedCan {
    incoming: mpsc::Receiver<Frame>,
    outgoing: UnboundedSender<Frame>,
}
struct Peer {
    incoming: mpsc::Sender<Frame>,
    outgoing: UnboundedReceiver<Frame>,
    extended: bool,
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

impl Peer {
    async fn next(&mut self, expected: [u8; 8]) {
        let frame = tokio::time::timeout(Duration::from_secs(2), self.outgoing.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.id(), Some(id(0x6ec, self.extended)));
        assert_eq!(frame.bus, 3);
        assert!(!frame.is_fd());
        assert_eq!(frame.data(), expected);
        let mut echo = frame;
        echo.loopback = true;
        self.incoming.send(echo).unwrap();
    }
    fn frame(&self, data: &[u8]) -> Frame {
        Frame::builder(id(0x6ed, self.extended))
            .bus(3)
            .data(data)
            .build()
            .unwrap()
    }
    fn reply(&self, data: &[u8]) {
        self.incoming.send(self.frame(data)).unwrap();
    }
    fn ack(&self, ctr: u8) {
        self.reply(&[0xff, 0, ctr, 0, 0, 0, 0, 0]);
    }
}

fn id(value: u32, extended: bool) -> Identifier {
    if extended {
        Identifier::extended(value).unwrap()
    } else {
        Identifier::standard(value).unwrap()
    }
}

fn client(order: ByteOrder, extended: bool, timeout: Duration) -> (CCPClient, Peer) {
    let (incoming, rx) = mpsc::channel();
    let (tx, outgoing) = unbounded_channel();
    let adapter = Arc::new(AsyncCanAdapter::new(ScriptedCan {
        incoming: rx,
        outgoing: tx,
    }));
    let mut can = CCPCanConfig::new(id(0x6ec, extended), id(0x6ed, extended));
    can.bus = 3;
    (
        CCPClient::with_config(
            CCPOnCAN::new(adapter, can).unwrap(),
            CCPConfig {
                byte_order: order,
                timeout,
            },
        )
        .unwrap(),
        Peer {
            incoming,
            outgoing,
            extended,
        },
    )
}

#[tokio::test]
async fn memory_commands_byte_order_station_order_and_response_filtering() {
    for (order, extended) in [
        (ByteOrder::LittleEndian, false),
        (ByteOrder::BigEndian, true),
    ] {
        let (client, mut peer) = client(order, extended, Duration::from_secs(1));
        let addr = match order {
            ByteOrder::LittleEndian => [0x00, 0x20, 0x00, 0x34],
            ByteOrder::BigEndian => [0x34, 0x00, 0x20, 0x00],
        };
        let post5 = match order {
            ByteOrder::LittleEndian => [0x05, 0x20, 0x00, 0x34],
            ByteOrder::BigEndian => [0x34, 0x00, 0x20, 0x05],
        };
        let post11 = match order {
            ByteOrder::LittleEndian => [0x0b, 0x20, 0x00, 0x34],
            ByteOrder::BigEndian => [0x34, 0x00, 0x20, 0x0b],
        };
        let task = tokio::spawn(async move {
            peer.next([0x01, 0, 0x34, 0x12, 0, 0, 0, 0]).await;
            let ack = [0xff, 0, 0, 0, 0, 0, 0, 0];
            let mut wrong = peer.frame(&ack);
            wrong.bus = 2;
            peer.incoming.send(wrong).unwrap();
            let wrong = Frame::builder(id(0x6ed, !extended))
                .bus(3)
                .data(&ack)
                .build()
                .unwrap();
            peer.incoming.send(wrong).unwrap();
            let wrong = Frame::builder(id(0x6ed, extended))
                .bus(3)
                .fd(true)
                .data(&ack)
                .build()
                .unwrap();
            peer.incoming.send(wrong).unwrap();
            peer.reply(&[0, 1, 2]); // DAQ messages may be shorter than eight bytes.
            peer.reply(&[0xfe, 1, 255, 0, 0, 0, 0, 0]); // Event warning has no matching CTR.
            peer.reply(&[0xff, 0, 255, 0, 0, 0, 0, 0]); // Stale CRM.
            peer.reply(&[0xff, 0x10, 0, 0, 0, 0, 0, 0]); // Busy must wait for the final ACK.
            peer.ack(0);
            peer.next([0x02, 1, 0, 2, addr[0], addr[1], addr[2], addr[3]])
                .await;
            peer.ack(1);
            peer.next([0x03, 2, 5, 0x10, 0x11, 0x12, 0x13, 0x14]).await;
            peer.reply(&[0xff, 0, 2, 2, post5[0], post5[1], post5[2], post5[3]]);
            peer.next([0x23, 3, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a])
                .await;
            peer.reply(&[0xff, 0, 3, 2, post11[0], post11[1], post11[2], post11[3]]);
            peer.next([0x02, 4, 0, 2, addr[0], addr[1], addr[2], addr[3]])
                .await;
            peer.ack(4);
            peer.next([0x04, 5, 5, 0, 0, 0, 0, 0]).await;
            peer.reply(&[0xff, 0, 5, 0x10, 0x11, 0x12, 0x13, 0x14]);
            peer.next([0x0f, 6, 4, 2, addr[0], addr[1], addr[2], addr[3]])
                .await;
            peer.reply(&[0xff, 0, 6, 0x10, 0x11, 0x12, 0x13, 0xaa]);
            peer.next([0x04, 7, 5, 0, 0, 0, 0, 0]).await; // SHORT_UP must not change MTA0.
            peer.reply(&[0xff, 0, 7, 0x15, 0x16, 0x17, 0x18, 0x19]);
            peer.next([0x07, 8, 0, 0, 0x34, 0x12, 0, 0]).await;
            peer.ack(8);
            peer.next([0x01, 9, 0x34, 0x12, 0, 0, 0, 0]).await;
            peer.ack(9);
            peer.next([0x07, 10, 1, 0, 0x34, 0x12, 0, 0]).await;
            peer.ack(10);
        });
        client.connect(0x1234).await.unwrap();
        client.set_mta(0x3400_2000, 2, 0).await.unwrap();
        let mta = client
            .download(&[0x10, 0x11, 0x12, 0x13, 0x14])
            .await
            .unwrap();
        assert_eq!(mta.address, 0x3400_2005);
        assert_eq!(mta.address_extension, 2);
        assert_eq!(
            client
                .download6(&[0x15, 0x16, 0x17, 0x18, 0x19, 0x1a])
                .await
                .unwrap()
                .address,
            0x3400_200b
        );
        client.set_mta(0x3400_2000, 2, 0).await.unwrap();
        assert_eq!(
            client.upload(5).await.unwrap(),
            [0x10, 0x11, 0x12, 0x13, 0x14]
        );
        assert_eq!(
            client.short_upload(0x3400_2000, 4, 2).await.unwrap(),
            [0x10, 0x11, 0x12, 0x13]
        );
        assert_eq!(
            client.upload(5).await.unwrap(),
            [0x15, 0x16, 0x17, 0x18, 0x19]
        );
        client.disconnect(true).await.unwrap();
        client.connect(0x1234).await.unwrap();
        client.disconnect(false).await.unwrap();
        task.await.unwrap();
    }
}

#[tokio::test]
async fn version_identification_seed_key_and_unknown_errors() {
    let (client, mut peer) = client(ByteOrder::LittleEndian, false, Duration::from_secs(1));
    let task = tokio::spawn(async move {
        peer.next([1, 0, 0, 0, 0, 0, 0, 0]).await;
        peer.ack(0);
        peer.next([0x1b, 1, 2, 1, 0, 0, 0, 0]).await;
        peer.reply(&[0xff, 0, 1, 2, 1, 0, 0, 0]);
        peer.next([0x17, 2, 1, 2, 3, 4, 5, 6]).await;
        peer.reply(&[0xff, 0, 2, 4, 2, 0x43, 1, 0]);
        peer.next([4, 3, 4, 0, 0, 0, 0, 0]).await;
        peer.reply(&[0xff, 0, 3, b'R', b'A', b'D', b'R', 0]);
        peer.next([0x12, 4, 1, 0, 0, 0, 0, 0]).await;
        peer.reply(&[0xff, 0, 4, 1, 0x14, 0x15, 0x16, 0x17]);
        peer.next([0x13, 5, 0x14, 0x15, 0x16, 0x17, 0, 0]).await;
        peer.reply(&[0xff, 0, 5, 1, 0, 0, 0, 0]);
        peer.next([4, 6, 1, 0, 0, 0, 0, 0]).await;
        peer.reply(&[0xff, 0xa5, 6, 1, 2, 3, 4, 5]);
        peer.next([4, 7, 1, 0, 0, 0, 0, 0]).await;
        peer.reply(&[0xff, 0, 7, 0x42, 0, 0, 0, 0]);
    });
    client.connect(0).await.unwrap();
    assert_eq!(client.get_ccp_version(2, 1).await.unwrap().minor, 1);
    let identity = client.exchange_id(&[1, 2, 3, 4, 5, 6]).await.unwrap();
    assert_eq!(
        (
            identity.length,
            identity.data_type,
            identity.resource_availability,
            identity.resource_protection
        ),
        (4, 2, 0x43, 1)
    );
    assert_eq!(client.upload(4).await.unwrap(), b"RADR");
    let seed = client.get_seed(Resource::Cal).await.unwrap();
    assert!(seed.protected);
    assert_eq!(seed.data, [0x14, 0x15, 0x16, 0x17]);
    assert_eq!(client.unlock(&seed.data).await.unwrap(), 1);
    assert!(
        matches!(client.upload(1).await,Err(automotive::Error::CCPError(Error::NegativeResponse { command:Command::Upload,code:ErrorCode::NonStandard(0xa5),data })) if data == [1,2,3,4,5])
    );
    assert_eq!(client.upload(1).await.unwrap(), [0x42]);
    task.await.unwrap();
}

#[tokio::test]
async fn counter_wrap_and_four_byte_short_up_encoding() {
    let (client, mut peer) = client(ByteOrder::LittleEndian, false, Duration::from_secs(1));
    let task = tokio::spawn(async move {
        peer.next([1, 0, 0, 0, 0, 0, 0, 0]).await;
        peer.ack(0);
        for i in 0..1024u32 {
            let a = (0x8000_0000 + i * 4).to_le_bytes();
            let ctr = (i + 1) as u8;
            peer.next([0x0f, ctr, 4, 0, a[0], a[1], a[2], a[3]]).await;
            peer.reply(&[0xff, 0, ctr, i as u8, 1, 2, 3, 0]);
        }
    });
    client.connect(0).await.unwrap();
    let mut bytes = Vec::new();
    for i in 0..1024u32 {
        bytes.extend(
            client
                .short_upload(0x8000_0000 + i * 4, 4, 0)
                .await
                .unwrap(),
        );
    }
    assert_eq!(bytes.len(), 4096);
    assert_eq!(&bytes[1024..1028], &[0, 1, 2, 3]);
    task.await.unwrap();
}

#[tokio::test]
async fn malformed_crm_requires_reconnect_with_fresh_counter() {
    for malformed in [vec![0xff, 0, 1], vec![]] {
        let (client, mut peer) = client(ByteOrder::LittleEndian, false, Duration::from_secs(1));
        let task = tokio::spawn(async move {
            peer.next([1, 0, 0, 0, 0, 0, 0, 0]).await;
            peer.ack(0);
            peer.next([4, 1, 1, 0, 0, 0, 0, 0]).await;
            peer.reply(&malformed);
            peer.next([1, 2, 0, 0, 0, 0, 0, 0]).await;
            peer.ack(1);
            peer.ack(2);
            peer.next([4, 3, 1, 0, 0, 0, 0, 0]).await;
            peer.reply(&[0xff, 0, 3, 0xab, 0, 0, 0, 0]);
        });
        client.connect(0).await.unwrap();
        assert!(matches!(
            client.upload(1).await,
            Err(automotive::Error::CCPError(Error::MalformedResponse { .. }))
        ));
        assert!(matches!(
            client.upload(1).await,
            Err(automotive::Error::CCPError(Error::ReconnectRequired))
        ));
        client.connect(0).await.unwrap();
        assert_eq!(client.upload(1).await.unwrap(), [0xab]);
        task.await.unwrap();
    }
}

#[tokio::test]
async fn cancelled_or_timed_out_write_is_never_retried() {
    for cancel in [false, true] {
        let (client, mut peer) = client(ByteOrder::LittleEndian, false, Duration::from_millis(100));
        let (started, active) = tokio::sync::oneshot::channel();
        let (resume, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            peer.next([1, 0, 0, 0, 0, 0, 0, 0]).await;
            peer.ack(0);
            peer.next([3, 1, 1, 0x42, 0, 0, 0, 0]).await;
            started.send(()).unwrap();
            ready.await.unwrap();
            peer.next([1, 2, 0, 0, 0, 0, 0, 0]).await;
            peer.ack(1);
            peer.ack(2);
        });
        client.connect(0).await.unwrap();
        let copy = client.clone();
        let write = tokio::spawn(async move { copy.download(&[0x42]).await });
        active.await.unwrap();
        if cancel {
            write.abort();
            assert!(write.await.unwrap_err().is_cancelled());
        } else {
            assert!(matches!(
                write.await.unwrap(),
                Err(automotive::Error::Timeout)
            ));
        }
        assert!(matches!(
            client.upload(1).await,
            Err(automotive::Error::CCPError(Error::ReconnectRequired))
        ));
        resume.send(()).unwrap();
        client.connect(0).await.unwrap();
        task.await.unwrap();
    }
}

#[tokio::test]
async fn memory_packet_limits_reject_unencodable_commands_before_transmission() {
    let (client, mut peer) = client(ByteOrder::LittleEndian, false, Duration::from_secs(1));
    let task = tokio::spawn(async move {
        peer.next([1, 0, 0, 0, 0, 0, 0, 0]).await;
        peer.ack(0);
        peer.next([4, 1, 1, 0, 0, 0, 0, 0]).await;
        peer.reply(&[0xff, 0, 1, 0xaa, 0, 0, 0, 0]);
    });
    client.connect(0).await.unwrap();
    for n in [0, 6, 256] {
        assert!(matches!(
            client.upload(n).await,
            Err(automotive::Error::CCPError(Error::InvalidArgument(_)))
        ));
        assert!(matches!(
            client.short_upload(0, n, 0).await,
            Err(automotive::Error::CCPError(Error::InvalidArgument(_)))
        ));
        assert!(matches!(
            client.download(&vec![0; n]).await,
            Err(automotive::Error::CCPError(Error::InvalidArgument(_)))
        ));
    }
    assert!(client.unlock(&[0; 7]).await.is_err());
    assert!(client.exchange_id(&[0; 7]).await.is_err());
    assert!(client.set_mta(0, 0, 2).await.is_err());
    assert!(client.get_seed(3).await.is_err());
    assert_eq!(client.upload(1).await.unwrap(), [0xaa]);
    task.await.unwrap();
}
