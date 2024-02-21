use automotive::async_can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::error::Error;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::panda::Panda;
use automotive::uds::constants::DataIdentifier;
use automotive::uds::UDSClient;

use bstr::ByteSlice;

async fn get_version(adapter: &AsyncCanAdapter, identifier: u32) -> Result<(), Error> {
    let config = IsoTPConfig::new(0, Identifier::Standard(identifier));
    let isotp = IsoTPAdapter::new(adapter, config);
    let uds = UDSClient::new(&isotp);

    let did = DataIdentifier::ApplicationSoftwareIdentification;
    let resp = uds.read_data_by_identifier(did as u16).await?;
    println!(
        "{:x} 0x{:x} {:?}: {:?}",
        identifier,
        did as u16,
        did,
        resp.as_bstr()
    );

    Ok(())
}

#[tokio::main]
async fn main() {
    // tracing_subscriber::fmt::init();

    let adapter = Panda::new().unwrap();
    let ids = 0x700..=0x7ff;

    let r = ids.map(|id| get_version(&adapter, id));
    futures::future::join_all(r).await;
}
