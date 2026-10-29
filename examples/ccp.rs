//! Dump the first 4 KiB of ECU flash using CCP SHORT_UP.

#[tokio::main]
async fn main() -> automotive::Result<()> {
    use automotive::{
        can::Identifier,
        protocols::ccp::{ByteOrder, CCPCanConfig, CCPClient, CCPConfig, CCPOnCAN},
        utils::hexdump,
    };
    use std::sync::Arc;

    let can = CCPCanConfig::new(Identifier::standard(0x6ec)?, Identifier::standard(0x6ed)?);
    let config = CCPConfig {
        byte_order: ByteOrder::LittleEndian,
        ..Default::default()
    };
    let adapter = Arc::new(automotive::adapters::get_adapter()?);

    // Connect to CCP
    let client = CCPClient::with_config(CCPOnCAN::new(adapter, can)?, config)?;
    client.connect(0).await?;

    // Read using SHORT_UP
    let start = 0x8000_0000;
    let mut data = Vec::with_capacity(0x1000);
    while data.len() < 0x1000 {
        data.extend(client.short_upload(start + data.len() as u32, 4, 0).await?);
    }

    // Print all data
    println!("{}", hexdump(&data, start, 1));
    client.disconnect(false).await?;
    client.close().await;
    Ok(())
}
