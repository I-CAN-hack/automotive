use automotive::uds::constants::{DataIdentifier, SessionType};
use bstr::ByteSlice;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let adapter = automotive::adapter::get_adapter()?;
    let isotp = automotive::isotp::IsoTPAdapter::from_id(&adapter, 0x7a1);
    let uds = automotive::uds::UDSClient::new(&isotp);

    uds.tester_present().await?;
    uds.diagnostic_session_control(SessionType::ExtendedDiagnostic as u8).await?;

    let did = DataIdentifier::ApplicationSoftwareIdentification;
    let resp = uds.read_data_by_identifier(did as u16).await?;

    // ApplicationSoftwareIdentification: "\x018965B4209000\0\0\0\0"
    println!("{:?}: {:?}", did, resp.as_bstr());

    Ok(())
}
