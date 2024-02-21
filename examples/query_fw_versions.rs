use automotive::can::Identifier;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::panda::Panda;
use automotive::uds::UDSClient;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = Panda::new().unwrap();

    let config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    let isotp = IsoTPAdapter::new(&adapter, config);
    let uds = UDSClient::new(&isotp);

    uds.tester_present().await.unwrap();
}
