use std::time::Duration;

use automotive::can::AsyncCanAdapter;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};
use automotive::Result;

use automotive::uds::DataIdentifier;
use automotive::uds::UDSClient;

use bstr::ByteSlice;
use strum::IntoEnumIterator;

async fn get_version(adapter: &AsyncCanAdapter, identifier: u32) -> Result<()> {
    let mut config = IsoTPConfig::new(0, identifier.into());

    // Increased timeout for adapters not supporting real ACKs
    // We send a lot of frames, so the timeout might start counting before the relevant frame is sent
    config.timeout = Duration::from_secs(1);

    let isotp = IsoTPAdapter::new(adapter, config);
    let uds = UDSClient::new(&isotp);

    uds.tester_present().await?;

    for did in DataIdentifier::iter() {
        if let Ok(resp) = uds.read_data_by_identifier(did as u16).await {
            println!(
                "{:x} 0x{:x} {:?}: {:?}",
                identifier,
                did as u16,
                did,
                resp.as_bstr()
            );
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = automotive::can::get_adapter().unwrap();

    let standard_ids = 0x700..=0x7f7;
    let extended_ids = (0xb0..=0xff).map(|i| 0x18da0000 + (i << 8) + 0xf1);

    let ids: Vec<u32> = standard_ids.chain(extended_ids).collect();

    let r = ids.iter().map(|id| get_version(&adapter, *id));
    futures::future::join_all(r).await;
}
