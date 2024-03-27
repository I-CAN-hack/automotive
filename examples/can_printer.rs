use automotive::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = automotive::can::get_adapter().unwrap();
    let mut stream = adapter.recv();

    while let Some(frame) = stream.next().await {
        let id: u32 = frame.id.into();
        println!("[{}]\t0x{:x}\t{}", frame.bus, id, hex::encode(frame.data));
    }
}
