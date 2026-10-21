use std::collections::VecDeque;
use std::time::Duration;

use automotive::can::{AsyncCanAdapter, CanAdapter, Frame, Identifier};
use automotive::Result;

struct EchoAdapter {
    received: Vec<Frame>,
    echo: fn(&Frame) -> Vec<Frame>,
}

impl CanAdapter for EchoAdapter {
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
        for frame in frames.drain(..) {
            self.received.extend((self.echo)(&frame));
        }
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<Frame>> {
        Ok(std::mem::take(&mut self.received))
    }

    fn buffer_size(&self) -> Option<usize> {
        Some(1)
    }
}

fn echo_without_flags(frame: &Frame) -> Frame {
    let mut echo = Frame::new(frame.bus, frame.id().unwrap(), frame.data()).unwrap();
    echo.loopback = true;
    echo
}

#[tokio::test]
async fn confirmations_accept_lost_frame_type_and_flags() {
    let adapter = AsyncCanAdapter::new(EchoAdapter {
        received: Vec::new(),
        echo: |frame| vec![echo_without_flags(frame)],
    });
    let id = Identifier::standard(0x123).unwrap();
    let frames = [
        Frame::builder(id).bus(2).data([1]).build().unwrap(),
        Frame::builder(id).bus(2).fd(true).build().unwrap(),
        Frame::builder(id)
            .bus(2)
            .fd(true)
            .brs(true)
            .esi(true)
            .data([0xaa; 8])
            .build()
            .unwrap(),
        Frame::builder(id)
            .bus(2)
            .fd(true)
            .brs(true)
            .esi(true)
            .data([0xaa; 12])
            .build()
            .unwrap(),
        Frame::remote(2, id, 15).unwrap(),
    ];

    // The one-frame hardware window also checks that each confirmation
    // releases capacity for the next pending send.
    tokio::time::timeout(Duration::from_secs(1), async {
        for result in
            futures::future::join_all(frames.iter().map(|frame| adapter.send(frame))).await
        {
            result.unwrap();
        }
    })
    .await
    .expect("transmissions were not confirmed");
}

#[tokio::test]
async fn confirmations_accept_controller_supplied_flags() {
    let adapter = AsyncCanAdapter::new(EchoAdapter {
        received: Vec::new(),
        echo: |frame| {
            vec![Frame::builder(frame.id().unwrap())
                .bus(frame.bus)
                .loopback(true)
                .fd(true)
                .brs(true)
                .esi(true)
                .data(frame.data())
                .build()
                .unwrap()]
        },
    });
    let frame = Frame::builder(Identifier::extended(0x123).unwrap())
        .fd(true)
        .data([0xaa; 4])
        .build()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), adapter.send(&frame))
        .await
        .expect("transmission was not confirmed")
        .unwrap();
}

#[tokio::test]
async fn stale_echoes_do_not_consume_confirmations() {
    let adapter = AsyncCanAdapter::new(EchoAdapter {
        received: Vec::new(),
        echo: |frame| {
            let echo = echo_without_flags(frame);
            if frame.data() == [1] {
                let mut wrong_bus = echo.clone();
                wrong_bus.bus += 1;
                let mut wrong_id =
                    Frame::new(0, Identifier::extended(0x123).unwrap(), &[1]).unwrap();
                wrong_id.loopback = true;
                let mut wrong_payload = Frame::new(0, frame.id().unwrap(), &[2]).unwrap();
                wrong_payload.loopback = true;
                vec![wrong_bus, wrong_id, wrong_payload, echo]
            } else {
                vec![echo]
            }
        },
    });
    let id = Identifier::standard(0x123).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        adapter
            .send(&Frame::new(0, id, &[1]).unwrap())
            .await
            .unwrap();
        adapter
            .send(&Frame::new(0, id, &[2]).unwrap())
            .await
            .unwrap();
    })
    .await
    .expect("stale echo consumed a pending confirmation");
}
