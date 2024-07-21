use automotive::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = automotive::can::get_adapter().unwrap();
    let mut stream = adapter.recv();

    while let Some(frame) = stream.next().await {
        println!("{:?}", frame);
    }
}
