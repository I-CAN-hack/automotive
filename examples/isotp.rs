use automotive::can::Identifier;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use futures_util::stream::StreamExt;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = automotive::can::get_adapter().unwrap();
    let config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    let isotp = IsoTPAdapter::new(&adapter, config);

    let mut stream = isotp.recv();

    isotp.send(&[0x3e, 0x00]).await.unwrap();
    let response = stream.next().await.unwrap().unwrap();
    println!("RX: {}", hex::encode(response));

    isotp.send(&[0x22, 0xf1, 0x81]).await.unwrap();
    let response = stream.next().await.unwrap().unwrap();
    println!("RX: {}", hex::encode(response));

    let mut long_request: [u8; 32] = [0; 32];
    long_request[0] = 0x10;
    isotp.send(&long_request).await.unwrap();
    let response = stream.next().await.unwrap().unwrap();
    println!("RX: {}", hex::encode(response));

    let long_response = [0x22, 0xf1, 0x81];
    isotp.send(&long_response).await.unwrap();
    let response = stream.next().await.unwrap().unwrap();
    println!("RX: {}", hex::encode(response));
}
