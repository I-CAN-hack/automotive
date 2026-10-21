use automotive::can::Identifier;
use automotive::transports::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::{Error, StreamExt};
use tracing_subscriber;

#[tokio::main]
async fn main() -> automotive::Result<()> {
    tracing_subscriber::fmt::init();

    let adapter = automotive::adapters::get_adapter()?;
    let config = IsoTPConfig::new(0, Identifier::standard(0x7a1)?);
    let isotp = IsoTPAdapter::new(&adapter, config);

    let mut stream = isotp.recv();

    isotp.send(&[0x3e, 0x00]).await?;
    let response = stream.next().await.ok_or(Error::Disconnected)??;
    println!("RX: {}", hex::encode(response));

    isotp.send(&[0x22, 0xf1, 0x81]).await?;
    let response = stream.next().await.ok_or(Error::Disconnected)??;
    println!("RX: {}", hex::encode(response));

    let mut long_request: [u8; 32] = [0; 32];
    long_request[0] = 0x10;
    isotp.send(&long_request).await?;
    let response = stream.next().await.ok_or(Error::Disconnected)??;
    println!("RX: {}", hex::encode(response));

    let long_response = [0x22, 0xf1, 0x81];
    isotp.send(&long_response).await?;
    let response = stream.next().await.ok_or(Error::Disconnected)??;
    println!("RX: {}", hex::encode(response));

    Ok(())
}
