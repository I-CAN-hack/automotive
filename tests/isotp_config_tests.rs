use automotive::can::{AsyncCanAdapter, CanAdapter, Frame, Identifier};
use automotive::transports::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::StreamExt;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

fn standard(id: u32) -> Identifier {
    Identifier::standard(id).unwrap()
}

#[test]
fn automatic_identifiers_and_defaults() {
    for (tx, rx) in [
        (standard(0), standard(8)),
        (standard(0x7a1), standard(0x7a9)),
        (
            Identifier::extended(0x18da10f1).unwrap(),
            Identifier::extended(0x18daf110).unwrap(),
        ),
    ] {
        let config = IsoTPConfig::builder(tx).build().unwrap();
        assert_eq!(config.tx_id, tx);
        assert_eq!(config.rx_id, rx);
        assert_eq!(config.bus, 0);
        assert_eq!(config.padding, Some(0xcc));
        assert_eq!(config.timeout, Duration::from_millis(100));
        assert_eq!(config.separation_time_min, None);
        assert!(!config.fd);
        assert_eq!(config.max_dlen, None);
        assert_eq!(config.ext_address, None);
        assert_eq!(config.rx_ext_address, None);
        assert_eq!(config.block_size, 0);
        assert_eq!(config.receive_separation_time_min, Duration::ZERO);
    }
}

#[test]
fn explicit_receive_ids_and_offsets_override_derivation_without_panicking() {
    assert!(IsoTPConfig::builder(standard(0x7f7)).build().is_ok());
    assert!(IsoTPConfig::builder(standard(0x7f8)).build().is_err());
    for offset in [8, u32::MAX] {
        assert!(IsoTPConfig::builder(standard(0x7ff))
            .rx_id_offset(offset)
            .build()
            .is_err());
    }
    let config = IsoTPConfig::builder(standard(0x7ff))
        .rx_id(standard(0x123))
        .build()
        .unwrap();
    assert_eq!(config.rx_id, standard(0x123));
    let config = IsoTPConfig::builder(standard(0x700))
        .rx_id(standard(0x123))
        .rx_id_offset(0x10)
        .build()
        .unwrap();
    assert_eq!(config.rx_id, standard(0x710));
    let config = IsoTPConfig::builder(standard(0x700))
        .rx_id_offset(u32::MAX)
        .rx_id(standard(0x123))
        .build()
        .unwrap();
    assert_eq!(config.rx_id, standard(0x123));
    assert!(IsoTPConfig::builder(Identifier::extended(0x123).unwrap())
        .rx_id_offset(8)
        .build()
        .is_err());
}

#[test]
fn tx_dl_accepts_only_specified_can_and_fd_lengths_regardless_of_setter_order() {
    for length in 0..=65 {
        assert_eq!(
            IsoTPConfig::builder(standard(0x700))
                .max_dlen(length)
                .build()
                .is_ok(),
            length == 8
        );
        let expected = [8, 12, 16, 20, 24, 32, 48, 64].contains(&length);
        assert_eq!(
            IsoTPConfig::builder(standard(0x700))
                .max_dlen(length)
                .fd(true)
                .build()
                .is_ok(),
            expected
        );
        assert_eq!(
            IsoTPConfig::builder(standard(0x700))
                .fd(true)
                .max_dlen(length)
                .build()
                .is_ok(),
            expected
        );
    }
    assert!(IsoTPConfig::builder(standard(0x700))
        .fd(true)
        .max_dlen(12)
        .fd(false)
        .build()
        .is_err());
}

#[test]
fn receive_stmin_rounds_up_and_rejects_unrepresentable_delays() {
    for (ns, expected_us) in [
        (0, 0),
        (1, 100),
        (100_001, 200),
        (900_001, 1000),
        (1_000_001, 2000),
        (127_000_000, 127_000),
    ] {
        let config = IsoTPConfig::builder(standard(0x700))
            .receive_separation_time_min(Duration::from_nanos(ns))
            .build()
            .unwrap();
        assert_eq!(
            config.receive_separation_time_min,
            Duration::from_micros(expected_us)
        );
    }
    assert!(IsoTPConfig::builder(standard(0x700))
        .receive_separation_time_min(Duration::from_nanos(127_000_001))
        .build()
        .is_err());
    assert!(IsoTPConfig::builder(standard(0x700))
        .timeout(Duration::ZERO)
        .build()
        .is_err());
    assert!(IsoTPConfig::builder(standard(0x700))
        .rx_ext_address(0x10)
        .build()
        .is_err());
}

// A CAN peer controlled by the test, including transmit confirmations. This
// exercises ISO-TP without depending on hardware or a host vcan interface.
struct TestCan {
    incoming: mpsc::Receiver<Frame>,
    outgoing: UnboundedSender<Frame>,
    echoes: Vec<Frame>,
}

impl CanAdapter for TestCan {
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> automotive::Result<()> {
        for mut frame in frames.drain(..) {
            self.outgoing.send(frame.clone()).unwrap();
            frame.loopback = true;
            self.echoes.push(frame);
        }
        Ok(())
    }

    fn recv(&mut self) -> automotive::Result<Vec<Frame>> {
        let mut frames = std::mem::take(&mut self.echoes);
        frames.extend(self.incoming.try_iter());
        Ok(frames)
    }
}

fn test_can() -> (
    AsyncCanAdapter,
    mpsc::Sender<Frame>,
    UnboundedReceiver<Frame>,
) {
    let (tx, rx) = mpsc::channel();
    let (outgoing, received) = unbounded_channel();
    let adapter = AsyncCanAdapter::new(TestCan {
        incoming: rx,
        outgoing,
        echoes: Vec::new(),
    });
    (adapter, tx, received)
}

#[tokio::test]
async fn receive_uses_configured_address_bus_block_size_and_stmin() {
    let (adapter, peer, mut sent) = test_can();
    let config = IsoTPConfig::builder(standard(0x700))
        .bus(2)
        .ext_address(0xf1)
        .rx_ext_address(0x10)
        .without_padding()
        .block_size(2)
        .receive_separation_time_min(Duration::from_micros(101))
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let mut stream = isotp.recv();
    let exchange = async {
        let ff = [0x10, 0x10, 23, 1, 2, 3, 4, 5];
        peer.send(Frame::new(1, config.rx_id, &ff).unwrap())
            .unwrap();
        let mut wrong_address = ff;
        wrong_address[0] = 0xf1;
        peer.send(Frame::new(2, config.rx_id, &wrong_address).unwrap())
            .unwrap();
        peer.send(Frame::new(2, config.rx_id, &ff).unwrap())
            .unwrap();
        let fc = sent.recv().await.unwrap();
        assert_eq!(fc.id(), Some(config.tx_id));
        assert_eq!(fc.bus, 2);
        assert_eq!(fc.data(), [0xf1, 0x30, 2, 0xf2]);
        for (sn, first) in [(1, 6), (2, 12)] {
            let mut data = vec![0x10, 0x20 | sn];
            data.extend(first..first + 6);
            peer.send(Frame::new(2, config.rx_id, &data).unwrap())
                .unwrap();
        }
        assert_eq!(sent.recv().await.unwrap().data(), fc.data());
        peer.send(Frame::new(2, config.rx_id, &[0x10, 0x23, 18, 19, 20, 21, 22, 23]).unwrap())
            .unwrap();
    };
    let (response, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(async { stream.next().await.unwrap().unwrap() }, exchange)
    })
    .await
    .unwrap();
    assert_eq!(response, (1..=23).collect::<Vec<u8>>());
    assert!(sent.try_recv().is_err(), "no FC after the final block");
}

#[tokio::test]
async fn send_obeys_each_peers_block_size_and_updated_stmin() {
    let (adapter, peer, mut sent) = test_can();
    let config = IsoTPConfig::builder(standard(0x700))
        .bus(2)
        .ext_address(0xf1)
        .rx_ext_address(0x10)
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    let isotp = IsoTPAdapter::new(&adapter, config);
    let exchange = async {
        assert_eq!(sent.recv().await.unwrap().data()[1] & 0xf0, 0x10);
        peer.send(Frame::new(2, config.rx_id, &[0x10, 0x30, 1, 0]).unwrap())
            .unwrap();
        assert_eq!(sent.recv().await.unwrap().data()[1], 0x21);
        peer.send(Frame::new(2, config.rx_id, &[0x10, 0x30, 2, 30]).unwrap())
            .unwrap();
        assert_eq!(sent.recv().await.unwrap().data()[1], 0x22);
        let start = std::time::Instant::now();
        assert_eq!(sent.recv().await.unwrap().data()[1], 0x23);
        assert!(start.elapsed() >= Duration::from_millis(29));
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        let (result, ()) = tokio::join!(isotp.send(&[0xcc; 23]), exchange);
        result.unwrap();
    })
    .await
    .unwrap();
}
