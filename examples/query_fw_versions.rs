use automotive::can::AsyncCanAdapter;
use automotive::can::Identifier;
use automotive::error::Error;
use automotive::isotp::{IsoTPAdapter, IsoTPConfig};

use automotive::uds::constants::DataIdentifier;
use automotive::uds::UDSClient;

use bstr::ByteSlice;
use strum::IntoEnumIterator;

static BUS: u8 = 0;
static ADDRS_IN_PARALLEL: usize = 128;

async fn get_version(adapter: &AsyncCanAdapter, identifier: u32) -> Result<(), Error> {
    let config = if identifier < 0x800 {
        IsoTPConfig::new(BUS, Identifier::Standard(identifier))
    } else {
        IsoTPConfig::new(BUS, Identifier::Extended(identifier))
    };

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

    let standard_ids = 0x700..=0x7ff;
    let extended_ids = (0x00..=0xff).map(|i| 0x18da0000 + (i << 8) + 0xf1);

    let ids: Vec<u32> = standard_ids.chain(extended_ids).collect();

    for ids in ids.chunks(ADDRS_IN_PARALLEL) {
        let r = ids.iter().map(|id| get_version(&adapter, *id));
        futures::future::join_all(r).await;
    }
}
