use automotive::uds::constants::DataIdentifier;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let adapter = automotive::adapter::get_adapter().unwrap();
    let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
    let uds = automotive::uds::UDSClient::new(&isotp);

    uds.tester_present().await.unwrap();
    let response = uds
        .read_data_by_identifier(DataIdentifier::ApplicationSoftwareIdentification as u16)
        .await
        .unwrap();

    println!(
        "Application Software Identification: {}",
        hex::encode(response)
    );
}
