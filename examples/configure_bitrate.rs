#[cfg(all(target_os = "windows", feature = "vector-xl"))]
pub async fn run() -> Result<()> {
    use automotive::can::bitrate::BitrateBuilder;
    use automotive::vector::VectorCan;
    tracing_subscriber::fmt::init();

    let bitrate_cfg = BitrateBuilder::new::<VectorCan>()
        .bitrate(500_000)
        .sample_point(0.8)
        .data_bitrate(2_000_000)
        .data_sample_point(0.8)
        .build()
        .unwrap();

    let adapter = VectorCan::new_async(0, &Some(bitrate_cfg.into()))?;
    let mut stream = adapter.recv();

    while let Some(frame) = stream.next().await {
        println!("{:?}", frame);
    }

    Ok(())
}

#[cfg(all(target_os = "windows", feature = "vector-xl"))]
#[tokio::main]
async fn main() -> automotive::Result<()> {
    run().await
}

#[cfg(not(all(target_os = "windows", feature = "vector-xl")))]
fn main() {
    eprintln!("This example requires Windows and the `vector-xl` feature.");
}
