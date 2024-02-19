use automotive::async_can::AsyncCanAdapter;
use automotive::panda::Panda;
use futures_util::stream::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let panda = Panda::new().unwrap();
    let async_can = AsyncCanAdapter::new(panda);

    let mut stream = async_can.recv();

    while let Some(frame) = stream.next().await {
        let id: u32 = frame.id.into();
        println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
    }
}
