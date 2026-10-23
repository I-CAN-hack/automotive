//! Configure a PEAK PCAN-USB FD adapter for 500 kbit/s CAN and 2 Mbit/s CAN FD.
//!
//! Run with `cargo run --example configure_bitrate --features peak`.

#[cfg(feature = "peak")]
pub async fn run() -> automotive::Result<()> {
    use automotive::adapters::peak::{PeakAdapter, PeakConfig};
    use automotive::StreamExt;

    tracing_subscriber::fmt::init();

    let config = PeakConfig::builder()
        .bitrate(|bitrate| {
            bitrate
                .bitrate(500_000)
                .sample_point(0.8)
                .data_bitrate(2_000_000)
                .data_sample_point(0.8)
        })
        .build()?;
    println!("Resolved timing: {:?}", config.bitrate);
    let adapter = PeakAdapter::new_async(config)?;
    let mut stream = adapter.recv();

    while let Some(frame) = stream.next().await {
        println!("{:?}", frame);
    }

    Ok(())
}

#[cfg(feature = "peak")]
#[tokio::main]
async fn main() -> automotive::Result<()> {
    run().await
}

#[cfg(not(feature = "peak"))]
fn main() -> automotive::Result<()> {
    eprintln!("This example requires the `peak` feature and a PEAK PCAN-USB FD adapter.");
    Ok(())
}
