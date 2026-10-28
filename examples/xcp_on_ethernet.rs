//! Read 4 KiB of ECU flash over Ethernet using SET_MTA and UPLOAD.

use std::time::Duration;

use automotive::protocols::xcp::{XCPClient, XCPConfig, XCPOnUDP};
use clap::Parser;

#[derive(Parser)]
#[command(about = "Read 4 KiB over XCP on Ethernet")]
struct Args {
    /// ECU host
    #[arg(long)]
    host: String,
    /// ECU UDP port
    #[arg(long)]
    port: u16,
    /// Local host to bind
    #[arg(long)]
    bind: String,
    /// Local UDP port to bind
    #[arg(long)]
    bind_port: u16,
    /// Start address, in hexadecimal or decimal
    #[arg(long, value_parser = parse_address)]
    address: u32,
}

fn parse_address(value: &str) -> Result<u32, std::num::ParseIntError> {
    match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => value.parse(),
    }
}

#[tokio::main]
async fn main() -> automotive::Result<()> {
    let args = Args::parse();
    let start = args.address;
    let timeout = Duration::from_secs(2);
    let transport = XCPOnUDP::bind_with_timeout(
        (args.bind.as_str(), args.bind_port),
        (args.host.as_str(), args.port),
        timeout,
    )
    .await?;
    let client = XCPClient::with_config(transport, XCPConfig { timeout })?;
    let info = client.connect(0).await?;
    println!("Connected: {info:?}");
    let chunk_size = info.max_upload_size();
    println!("Reading 4096 bytes at {start:#010x}, up to {chunk_size} bytes per request");

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
