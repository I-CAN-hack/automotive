use automotive::can::Identifier;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = automotive::adapter::get_adapter().unwrap();
    let config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    let isotp = IsoTPAdapter::new(&adapter, config);

    let response = isotp.recv();
    isotp.send(&[0x3e, 0x00]).await.unwrap();
    let response = response.await.unwrap();
    println!("RX: {}", hex::encode(response));

    let response = isotp.recv();
    isotp.send(&[0x22, 0xf1, 0x81]).await.unwrap();
    let response = response.await.unwrap();
    println!("RX: {}", hex::encode(response));

    let mut long_request: [u8; 32] = [0; 32];
    long_request[0] = 0x10;
    let response = isotp.recv();
    isotp.send(&long_request).await.unwrap();
    let response = response.await.unwrap();
    println!("RX: {}", hex::encode(response));
}
