//! Read ECU VINs using active or passive DoIP discovery over IPv4 or IPv6.

use std::{net::Ipv4Addr, time::Duration};

use automotive::{
    protocols::uds::{DataIdentifier, UDSClient, UDSConfig},
    transports::doip::{
        discover_with_config, Discovery, DiscoveryConfig, DoIPConfig, VehicleAnnouncement,
    },
    Stream, StreamExt,
};
use clap::{error::ErrorKind, CommandFactory, Parser, ValueEnum};

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Active,
    Passive,
}

#[derive(Parser)]
#[command(about = "Read ECU VINs with DoIP discovery (Linux adapter selection)")]
struct Args {
    #[arg(long, value_enum, default_value = "active")]
    discovery: Mode,
    /// IPv6 interface (required with --ipv6)
    #[arg(long)]
    adapter: Option<String>,
    /// Local IPv4 address/prefix (required for active IPv4 discovery)
    #[arg(long)]
    local_address: Option<String>,
    /// Use IPv4 broadcast (default)
    #[arg(long, conflicts_with = "ipv6")]
    ipv4: bool,
    /// Use IPv6 multicast
    #[arg(long, requires = "adapter")]
    ipv6: bool,
    #[arg(long, default_value = "0x0E89", value_parser = parse_address)]
    source_address: u16,
}

fn parse_address(value: &str) -> Result<u16, std::num::ParseIntError> {
    match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(hex) => u16::from_str_radix(hex, 16),
        None => value.parse(),
    }
}

fn announcements(
    config: DiscoveryConfig,
    mode: Mode,
) -> impl Stream<Item = automotive::Result<VehicleAnnouncement>> {
    async_stream::try_stream! {
        match mode {
            Mode::Active => {
                for vehicle in discover_with_config(config).await? {
                    yield vehicle;
                }
            }
            Mode::Passive => {
                let listener = Discovery::bind(config.passive())?;
                println!("Listening for announcements; power up the ECU. Press Ctrl+C to stop.");
                loop {
                    yield listener.recv().await?;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !args.ipv6 && matches!(args.discovery, Mode::Active) && args.local_address.is_none() {
        Args::command()
            .error(
                ErrorKind::MissingRequiredArgument,
                "active IPv4 discovery requires --local-address",
            )
            .exit();
    }
    let mut local_address = None;
    let config = if args.ipv6 {
        let adapter = args.adapter.as_deref().unwrap();
        let index = std::fs::read_to_string(format!("/sys/class/net/{adapter}/ifindex"))?;
        DiscoveryConfig::ipv6(index.trim().parse()?)
    } else if matches!(args.discovery, Mode::Active) {
        let (address, prefix) = args
            .local_address
            .as_deref()
            .unwrap()
            .split_once('/')
            .unwrap();
        let address: Ipv4Addr = address.parse()?;
        let prefix: u32 = prefix.parse()?;
        let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
        let broadcast = Ipv4Addr::from(u32::from(address) | !mask);
        local_address = Some((address, 0).into());
        DiscoveryConfig {
            bind_address: local_address.unwrap(),
            destination: format!("{broadcast}:13400").parse()?,
            ..Default::default()
        }
    } else {
        DiscoveryConfig::default()
    };

    let vehicles = announcements(config, args.discovery);
    tokio::pin!(vehicles);
    while let Some(vehicle) = vehicles.next().await {
        let vehicle = vehicle?;
        let mut doip_config = DoIPConfig::new(vehicle.logical_address);
        doip_config.source_address = args.source_address;
        doip_config.protocol_version = vehicle.protocol_version;
        doip_config.local_address = local_address;
        let transport = vehicle.connect_with_config(doip_config).await?;
        let uds = UDSClient::with_config(
            &transport,
            UDSConfig::builder()
                .p2_timeout(Duration::from_secs(1))
                .build(),
        );
        let vin = uds.read_data_by_identifier(DataIdentifier::Vin).await?;
        println!(
            "{} 0x{:04x} VIN: {}",
            vehicle.address,
            vehicle.logical_address,
            String::from_utf8_lossy(&vin)
        );
    }
    Ok(())
}
