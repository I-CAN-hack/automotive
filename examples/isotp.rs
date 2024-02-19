use automotive::async_can::AsyncCanAdapter;
use automotive::can::{Frame, Identifier};
use automotive::panda::Panda;
use futures_util::stream::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let panda = Panda::new().unwrap();
    let async_can = AsyncCanAdapter::new(panda);

    // let mut stream = async_can.recv_filter(|frame| frame.id > Identifier::Standard(0x700));
    let mut stream = async_can.recv();

    let tester_present = Frame::new(0, 0x7a1, &[0x02, 0x3e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    async_can.send(&tester_present).await;
    println!("{:?}", tester_present);

    while let Some(frame) = stream.next().await {
        let id: u32 = frame.id.into();
        let tx_rx = if frame.returned { "TX" } else { "RX" };
        println!("[{}]\t{}\t0x{:x}\t{}", tx_rx, frame.bus, id, hex::encode(frame.data));
    }
}
