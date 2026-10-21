use automotive::protocols::uds::{DataIdentifier, SessionType};
use bstr::ByteSlice;

#[tokio::main]
async fn main() -> automotive::Result<()> {
    tracing_subscriber::fmt::init();

    let adapter = automotive::adapters::get_adapter()?;
    let isotp = automotive::transports::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
    let uds = automotive::protocols::uds::UDSClient::new(&isotp);

    uds.tester_present().await?;
    uds.diagnostic_session_control(SessionType::ExtendedDiagnostic)
        .await?;

    let did = DataIdentifier::ApplicationSoftwareIdentification;
    let resp = uds.read_data_by_identifier(did).await?;

    // ApplicationSoftwareIdentification: "\x018965B4209000\0\0\0\0"
    println!("{:?}: {:?}", did, resp.as_bstr());

    Ok(())
}
