//! Read 4 KiB of ECU flash using SET_MTA and UPLOAD.

#[tokio::main]
async fn main() -> automotive::Result<()> {
    use automotive::{
        can::{Frame, Identifier},
        protocols::xcp::{XCPCanConfig, XCPClient, XCPOnCAN},
    };
    use std::{sync::Arc, time::Duration};

    let adapter = Arc::new(automotive::adapters::get_adapter()?);

    // Wake ECU
    let wake = Frame::builder(Identifier::standard(0x541)?)
        .data(&[0xff; 8])
        .build()?;
    for _ in 0..10 {
        adapter.send(&wake).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let config = XCPCanConfig::new(Identifier::standard(0x6c3)?, Identifier::standard(0x6c4)?);
    // Set config.fd = true for an interface configured for CAN FD.
    let client = XCPClient::new(XCPOnCAN::new(adapter, config)?)?;
    let info = client.connect(0).await?;
    println!("Connected: {info:?}");
    let start = 0x8000_0000;
    let chunk_size = info.max_upload_size();

    // Read using UPLOAD
    client.set_mta(start, 0).await?;
    let mut data = Vec::new();
    while data.len() < 4096 {
        data.extend(client.upload(chunk_size.min(4096 - data.len())).await?);
    }

    // Print all data
    for (row, chunk) in data.chunks(16).enumerate() {
        let hex_data = chunk
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii_data: String = chunk
            .iter()
            .map(|&byte| {
                if (32..127).contains(&byte) {
                    char::from(byte)
                } else {
                    '.'
                }
            })
            .collect();
        let address = start + (row * 16 / usize::from(info.address_granularity)) as u32;
        println!("{address:08x}  {hex_data:<47}  |{ascii_data}|");
    }
    Ok(())
}
