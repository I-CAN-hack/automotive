use automotive::can::Identifier;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::panda::Panda;
use automotive::uds::constants::DataIdentifier;
use automotive::uds::UDSClient;
use bstr::ByteSlice;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = Panda::new().unwrap();

    let config = IsoTPConfig::new(0, Identifier::Standard(0x7a1));
    let isotp = IsoTPAdapter::new(&adapter, config);
    let uds = UDSClient::new(&isotp);

    uds.tester_present().await.unwrap();

    let did = DataIdentifier::ApplicationSoftwareIdentification;
    let resp = uds.read_data_by_identifier(did as u16).await.unwrap();
    println!("0x{:x} {:?}: {:?}", did as u16, did, resp.as_bstr());
}
