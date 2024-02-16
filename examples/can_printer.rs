use automotive::can::CanAdapter;
use automotive::panda::Panda;

fn main() {
    let mut panda = Panda::new().unwrap();
    println!("{:?}", panda.get_hw_type().unwrap());

    loop {
        let frames = panda.recv().unwrap();
        for frame in frames {
            let id: u32 = frame.id.into();
            println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
        }
    }
}
