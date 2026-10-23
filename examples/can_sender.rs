//! Send one Classic CAN frame through the first available adapter.
//!
//! Run with `cargo run --example can_sender --features default-adapters`.

use automotive::can::{Frame, Identifier};

#[tokio::main]
async fn main() -> automotive::Result<()> {
    let adapter = automotive::adapters::get_adapter()?;
    let frame = Frame::builder(Identifier::standard(0x123)?)
        .data(&[0x01, 0x02, 0x03, 0x04])
        .build()?;

    adapter.send(&frame).await?;
    println!("Sent {frame:?}");
    Ok(())
}
