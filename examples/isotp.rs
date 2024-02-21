use automotive::can::Identifier;
use automotive::isotp::{IsoTP, IsoTPConfig};
use automotive::panda::Panda;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = Panda::new().unwrap();

    let config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    let isotp = IsoTP::new(&adapter, config);

    let response = isotp.recv();
    isotp.send(&[0x3e, 0x00]).await.unwrap();
    response.await.unwrap();

    let response = isotp.recv();
    isotp.send(&[0x22, 0xf1, 0x81]).await.unwrap();
    response.await.unwrap();

    let mut long_request: [u8; 32] = [0; 32];
    long_request[0] = 0x10;
    let response = isotp.recv();
    isotp.send(&long_request).await.unwrap();
    response.await.unwrap();
}
