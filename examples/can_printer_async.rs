use automotive::async_can::AsyncCanAdapter;
use automotive::panda::Panda;
use tracing_subscriber;
use futures_util::stream::StreamExt;
use futures_util::pin_mut;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let panda = Panda::new().unwrap();
    let async_can = AsyncCanAdapter::new(panda);

    let stream = async_can.recv();
    pin_mut!(stream);

    while let Some(Ok(frame)) = stream.next().await {
        let id: u32 = frame.id.into();
        println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
    }
}
