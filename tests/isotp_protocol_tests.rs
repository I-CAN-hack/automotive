//! Scripted CAN peers exercise ISO-TP wire behavior, including traffic that a
//! conforming Scapy peer would not normally transmit.

use automotive::can::{AsyncCanAdapter, CanAdapter, Frame, Identifier};
use automotive::transports::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::StreamExt;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

struct SentFrame {
    frame: Frame,
    at: Instant,
}

struct ScriptedCan {
    incoming: mpsc::Receiver<Frame>,
    outgoing: UnboundedSender<SentFrame>,
}

impl CanAdapter for ScriptedCan {
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> automotive::Result<()> {
        for frame in frames.drain(..) {
            self.outgoing
                .send(SentFrame {
                    frame,
                    at: Instant::now(),
                })
                .unwrap();
        }
        Ok(())
    }

    fn recv(&mut self) -> automotive::Result<Vec<Frame>> {
        Ok(self.incoming.try_iter().collect())
    }
}

struct Peer {
    incoming: mpsc::Sender<Frame>,
    outgoing: UnboundedReceiver<SentFrame>,
}

impl Peer {
    fn inject(&self, frame: Frame) {
        self.incoming.send(frame).unwrap();
    }

    fn acknowledge(&self, frame: &Frame) {
        let mut echo = frame.clone();
        echo.loopback = true;
        self.inject(echo);
    }

    async fn next(&mut self) -> SentFrame {
        let sent = self.outgoing.recv().await.unwrap();
        self.acknowledge(&sent.frame);
        sent
    }

    async fn assert_no_transmission(&mut self) {
        assert!(
            tokio::time::timeout(Duration::from_millis(20), self.outgoing.recv())
                .await
                .is_err()
        );
    }
}

fn channel() -> (AsyncCanAdapter, Peer) {
    let (incoming, receiver) = mpsc::channel();
    let (outgoing, sent) = unbounded_channel();
    let adapter = AsyncCanAdapter::new(ScriptedCan {
        incoming: receiver,
        outgoing,
    });
    (
        adapter,
        Peer {
            incoming,
            outgoing: sent,
        },
    )
}

fn config() -> IsoTPConfig {
    IsoTPConfig::builder(Identifier::standard(0x700).unwrap())
        .bus(2)
        .without_padding()
        .timeout(Duration::from_millis(250))
        .build()
        .unwrap()
}

fn received(config: &IsoTPConfig, data: &[u8]) -> Frame {
    Frame::builder(config.rx_id)
        .bus(config.bus)
        .fd(config.fd)
        .data(data)
        .build()
        .unwrap()
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), future)
        .await
        .expect("protocol exchange stalled")
}

#[tokio::test]
async fn sender_applies_each_blocks_size_and_stmin() {
    let (adapter, mut peer) = channel();
    let config = config();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let exchange = async {
        assert_eq!(peer.next().await.frame.data()[0] & 0xf0, 0x10);
        peer.inject(received(&config, &[0x30, 1, 0]));
        assert_eq!(peer.next().await.frame.data()[0], 0x21);
        peer.assert_no_transmission().await;

        peer.inject(received(&config, &[0x30, 2, 30]));
        let second = peer.next().await;
        let third = peer.next().await;
        assert_eq!(second.frame.data()[0], 0x22);
        assert_eq!(third.frame.data()[0], 0x23);
        assert!(third.at.duration_since(second.at) >= Duration::from_millis(30));
        peer.assert_no_transmission().await;

        peer.inject(received(&config, &[0x30, 0, 1]));
        for sn in 4..=6 {
            assert_eq!(peer.next().await.frame.data()[0], 0x20 | sn);
        }
    };
    within(async {
        let (result, ()) = tokio::join!(isotp.send(&[0xaa; 48]), exchange);
        result.unwrap();
    })
    .await;
}

#[tokio::test]
async fn receive_ignores_other_buses_during_start_and_reassembly() {
    let (adapter, mut peer) = channel();
    let config = config();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let mut stream = isotp.recv();
    let exchange = async {
        let mut wrong_bus = received(&config, &[1, 0xee]);
        wrong_bus.bus = 1;
        peer.inject(wrong_bus);
        peer.inject(received(&config, &[0x10, 20, 1, 2, 3, 4, 5, 6]));
        assert_eq!(peer.next().await.frame.data(), [0x30, 0, 0]);
        let mut wrong_bus = received(&config, &[0x21, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee]);
        wrong_bus.bus = 1;
        peer.inject(wrong_bus);
        peer.inject(received(&config, &[0x21, 7, 8, 9, 10, 11, 12, 13]));
        peer.inject(received(&config, &[0x22, 14, 15, 16, 17, 18, 19, 20]));
    };
    let (message, ()) =
        within(async { tokio::join!(async { stream.next().await.unwrap().unwrap() }, exchange) })
            .await;
    assert_eq!(message, (1..=20).collect::<Vec<u8>>());
}

#[tokio::test]
async fn flow_control_on_another_bus_cannot_authorize_transmission() {
    let (adapter, mut peer) = channel();
    let config = config();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let exchange = async {
        assert_eq!(peer.next().await.frame.data()[0] & 0xf0, 0x10);
        let mut wrong_bus = received(&config, &[0x30, 0, 0]);
        wrong_bus.bus = 1;
        peer.inject(wrong_bus);
        peer.assert_no_transmission().await;
        peer.inject(received(&config, &[0x30, 0, 0]));
        assert_eq!(peer.next().await.frame.data(), [0x21, 0xaa, 0xaa]);
    };
    within(async {
        let (result, ()) = tokio::join!(isotp.send(&[0xaa; 8]), exchange);
        result.unwrap();
    })
    .await;
}
