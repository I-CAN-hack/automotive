use automotive::async_can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::panda::Panda;
use futures_util::stream::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let panda = Panda::new().unwrap();
    let async_can = AsyncCanAdapter::new(panda);

    // let mut stream = async_can.recv();
    let mut stream = async_can.recv_filter(|frame| frame.id == Identifier::Standard(0x30));

    while let Some(frame) = stream.next().await {
        let id: u32 = frame.id.into();
        println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
    }
}
