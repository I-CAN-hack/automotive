use automotive::async_can::AsyncCanAdapter;
use automotive::panda::Panda;

// use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    // Can this be cleaned up?
    let panda = Box::new(Panda::new().unwrap());
    let panda: &'static mut Panda = Box::leak(panda);

    let async_can = AsyncCanAdapter::new(panda);

    loop {
        let frame = async_can.recv().await.unwrap();
        let id: u32 = frame.id.into();
        println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
    }
}
