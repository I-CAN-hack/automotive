use automotive::can::CanAdapter;
use automotive::can::Frame;
use automotive::panda::Panda;

fn main() {
    let mut panda = Panda::new().unwrap();

    let tester_present = Frame::new(
        0,
        0x7a1.into(),
        &[0x02, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    );
    panda.send(&[tester_present]).unwrap();

    loop {
        let frames = panda.recv().unwrap();
        for frame in frames {
            let id: u32 = frame.id.into();
            if id < 0x700 {
                continue;
            }
            let tx_rx = if frame.returned { "TX" } else { "RX" };
            println!(
                "[{}] {}\t0x{:x}\t{}",
                tx_rx,
                frame.bus,
                id,
                hex::encode(frame.data)
            );
        }
    }
}
