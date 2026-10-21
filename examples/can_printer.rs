use automotive::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() -> automotive::Result<()> {
    tracing_subscriber::fmt::init();

    let adapter = automotive::adapters::get_adapter()?;
    let mut stream = adapter.recv();

    while let Some(frame) = stream.next().await {
        println!("{:?}", frame);
    }

    Ok(())
}
