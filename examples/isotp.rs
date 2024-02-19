use automotive::async_can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::isotp::{IsoTP, IsoTPConfig};
use automotive::panda::Panda;
use futures_util::stream::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let panda = Panda::new().unwrap();
    let async_can = AsyncCanAdapter::new(panda);

    // Debug stream
    let mut stream = async_can.recv_filter(|frame| frame.id > Identifier::Standard(0x700));

    let config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    let isotp = IsoTP::new(&async_can, config);

    let response = isotp.recv();
    isotp.send(&[0x3e, 0x00]).await;
    response.await.unwrap();

    let response = isotp.recv();
    isotp.send(&[0x22, 0xf1, 0x81]).await;
    response.await.unwrap();

    // Print all frames for debugging
    while let Some(frame) = stream.next().await {
        let id: u32 = frame.id.into();
        let tx_rx = if frame.returned { "TX" } else { "RX" };
        println!(
            "[{}]\t{}\t0x{:x}\t{}",
            tx_rx,
            frame.bus,
            id,
            hex::encode(frame.data)
        );
    }
}
