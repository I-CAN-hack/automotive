//! Identify vehicles and read a DID using UDS over DoIP.
use automotive::{protocols::uds::UDSClient, transports::doip::discover};

#[tokio::main]
async fn main() -> automotive::Result<()> {
    let vehicles = discover().await?;
    for vehicle in vehicles {
        println!(
            "{} VIN {}",
            vehicle.address,
            String::from_utf8_lossy(&vehicle.vin)
        );
        let transport = vehicle.connect().await?;
        let uds = UDSClient::new(&transport);
        println!("VIN: {:?}", uds.read_data_by_identifier(0xf190).await?);
    }
    Ok(())
}
