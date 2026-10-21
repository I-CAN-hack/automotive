//! Scripted CAN peers exercise ISO-TP wire behavior, including traffic that a
//! conforming Scapy peer would not normally transmit.

use automotive::can::{AsyncCanAdapter, CanAdapter, Frame, Identifier};
use automotive::transports::isotp::{Error as IsoTPError, IsoTPAdapter, IsoTPConfig};
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

#[tokio::test]
async fn invalid_start_frames_are_ignored_without_flow_control() {
    for (fd, address) in [
        (false, None),
        (false, Some(0x55)),
        (true, None),
        (true, Some(0x55)),
    ] {
        let (adapter, mut peer) = channel();
        let mut config = config();
        config.fd = fd;
        config.ext_address = address;
        let isotp = IsoTPAdapter::new(&adapter, config);
        let mut stream = isotp.recv();
        for invalid in [
            vec![],
            vec![0],
            vec![0x10],
            vec![0x10, 0],
            vec![0, 1, 0xaa],
            vec![0x10, 1, 0, 0, 0, 0, 0, 0],
            vec![0x10, 0, 0, 0, 0x0f, 0xff, 0xaa, 0xbb],
        ] {
            let mut data: Vec<_> = address.into_iter().chain(invalid).collect();
            data.truncate(8);
            peer.inject(received(&config, &data));
        }
        let valid: Vec<_> = address.into_iter().chain([1, 0xab]).collect();
        peer.inject(received(&config, &valid));
        assert_eq!(within(stream.next()).await.unwrap().unwrap(), [0xab]);
        peer.assert_no_transmission().await;
    }
}

#[tokio::test]
async fn fd_single_frames_enforce_escape_encoding_and_dlc_length_ranges() {
    // ISO 15765-2:2016, Table 13: smallest SF_DL for each selected CAN_DL.
    for (can_dl, minimum) in [(12, 8), (16, 11), (64, 47)] {
        let (adapter, peer) = channel();
        let mut config = config();
        config.fd = true;
        let isotp = IsoTPAdapter::new(&adapter, config);
        let mut stream = isotp.recv();
        let mut invalid = vec![0xaa; can_dl];
        invalid[0] = 1; // Non-escape encoding in a frame larger than eight bytes.
        peer.inject(received(&config, &invalid));
        invalid[0] = 0;
        invalid[1] = minimum - 1;
        peer.inject(received(&config, &invalid));
        invalid[1] = (can_dl - 1) as u8; // SF_DL exceeds the available payload.
        peer.inject(received(&config, &invalid));
        invalid[0] = 0x10;
        invalid[1] = (can_dl - 2) as u8; // A FirstFrame for a message fitting an SF.
        peer.inject(received(&config, &invalid));
        let mut valid = vec![0xab; can_dl];
        valid[0] = 0;
        valid[1] = minimum;
        peer.inject(received(&config, &valid));
        assert_eq!(
            within(stream.next()).await.unwrap().unwrap(),
            vec![0xab; usize::from(minimum)]
        );
    }
}

#[tokio::test]
async fn reassembly_learns_peer_rx_dl_and_ignores_wrong_cf_lengths() {
    for (tx_dl, rx_dl, length) in [(64, 8, 20), (8, 16, 34)] {
        let (adapter, mut peer) = channel();
        let mut config = config();
        config.fd = true;
        config.max_dlen = Some(tx_dl);
        let isotp = IsoTPAdapter::new(&adapter, config);
        let mut stream = isotp.recv();
        let message: Vec<u8> = (1..=length).collect();
        let exchange = async {
            let mut first = vec![0x10, length];
            first.extend_from_slice(&message[..rx_dl - 2]);
            peer.inject(received(&config, &first));
            assert_eq!(peer.next().await.frame.data(), [0x30, 0, 0]);
            let mut invalid = vec![0xee; if rx_dl == 8 { 12 } else { 8 }];
            invalid[0] = 0x21;
            peer.inject(received(&config, &invalid));
            for (index, chunk) in message[rx_dl - 2..].chunks(rx_dl - 1).enumerate() {
                let sn = 0x21 + index as u8;
                if rx_dl == 16 && index == 1 {
                    let mut excessive_padding = vec![0xee; 12];
                    excessive_padding[0] = sn;
                    peer.inject(received(&config, &excessive_padding));
                }
                let mut cf = vec![sn];
                cf.extend_from_slice(chunk);
                peer.inject(received(&config, &cf));
            }
        };
        let (response, ()) = within(async {
            tokio::join!(async { stream.next().await.unwrap().unwrap() }, exchange)
        })
        .await;
        assert_eq!(response, message);
    }
}

#[tokio::test]
async fn idle_consecutive_and_unknown_frames_do_not_interrupt_the_next_message() {
    let (adapter, mut peer) = channel();
    let config = config();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let mut stream = isotp.recv();
    for data in [&[0x21, 0xee][..], &[0xf0], &[0x30, 0, 0], &[1, 0xab]] {
        peer.inject(received(&config, data));
    }
    assert_eq!(within(stream.next()).await.unwrap().unwrap(), [0xab]);
    peer.assert_no_transmission().await;
}

#[tokio::test]
async fn replacement_first_frame_reports_interruption_and_restarts_reassembly() {
    let (adapter, mut peer) = channel();
    let config = config();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let mut stream = isotp.recv();
    let exchange = async {
        peer.inject(received(&config, &[0x10, 27, 1, 2, 3, 4, 5, 6]));
        peer.next().await;
        peer.inject(received(&config, &[0x21, 7, 8, 9, 10, 11, 12, 13]));
        peer.inject(received(&config, &[0x10, 13, 40, 41, 42, 43, 44, 45]));
        assert_eq!(peer.next().await.frame.data(), [0x30, 0, 0]);
        peer.inject(received(&config, &[0x21, 46, 47, 48, 49, 50, 51, 52]));
    };
    let receive = async {
        assert!(matches!(
            stream.next().await.unwrap(),
            Err(automotive::Error::IsoTPError(IsoTPError::UnexpectedPdu))
        ));
        assert_eq!(
            stream.next().await.unwrap().unwrap(),
            (40..=52).collect::<Vec<u8>>()
        );
    };
    within(async { tokio::join!(receive, exchange) }).await;
}

#[tokio::test]
async fn replacement_single_frame_preserves_the_interruption_indication_and_message() {
    let (adapter, mut peer) = channel();
    let config = config();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let mut stream = isotp.recv();
    let exchange = async {
        peer.inject(received(&config, &[0x10, 20, 1, 2, 3, 4, 5, 6]));
        peer.next().await;
        peer.inject(received(&config, &[0x10, 1, 0, 0, 0, 0, 0, 0])); // Invalid FF: ignore.
        peer.inject(received(&config, &[0])); // Invalid SF: ignore.
        peer.inject(received(&config, &[1, 0xab]));
    };
    let receive = async {
        assert!(matches!(
            stream.next().await.unwrap(),
            Err(automotive::Error::IsoTPError(IsoTPError::UnexpectedPdu))
        ));
        assert_eq!(stream.next().await.unwrap().unwrap(), [0xab]);
    };
    within(async { tokio::join!(receive, exchange) }).await;
}

#[tokio::test]
async fn waits_restart_flow_control_deadline_without_a_sender_side_limit() {
    let (adapter, mut peer) = channel();
    let mut config = config();
    config.timeout = Duration::from_millis(100);
    let isotp = IsoTPAdapter::new(&adapter, config);
    let exchange = async {
        peer.next().await;
        // Total WAIT time exceeds N_Bs, and more than ten WAITs are valid.
        for _ in 0..11 {
            tokio::time::sleep(Duration::from_millis(15)).await;
            peer.inject(received(&config, &[0x31, 0xff, 0xff]));
        }
        peer.inject(received(&config, &[0x30, 0, 0]));
        assert_eq!(peer.next().await.frame.data()[0], 0x21);
    };
    within(async {
        let (result, ()) = tokio::join!(isotp.send(&[0xaa; 8]), exchange);
        result.unwrap();
    })
    .await;
}

#[tokio::test]
async fn reserved_stmin_uses_127ms_for_the_rest_of_the_transfer() {
    let (adapter, mut peer) = channel();
    let config = config();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let exchange = async {
        peer.next().await;
        peer.inject(received(&config, &[0x30, 1, 0x80]));
        let first = peer.next().await;
        peer.inject(received(&config, &[0x30, 0, 0]));
        let second = peer.next().await;
        let third = peer.next().await;
        assert!(second.at.duration_since(first.at) >= Duration::from_millis(127));
        assert!(third.at.duration_since(second.at) >= Duration::from_millis(127));
    };
    within(async {
        let (result, ()) = tokio::join!(isotp.send(&[0xaa; 27]), exchange);
        result.unwrap();
    })
    .await;
}
