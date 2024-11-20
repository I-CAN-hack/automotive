use automotive::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = automotive::vector::VectorCan::new_async().unwrap();
    let frame = automotive::can::Frame::new(0, 0x123.into(), &[0xAA, 0xAA]).unwrap();

    adapter.send(&frame).await;

    let mut stream = adapter.recv();

    while let Some(frame) = stream.next().await {
        println!("{:?}", frame);
    }
}
