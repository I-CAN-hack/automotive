#[cfg(all(target_os = "windows", feature = "j2534"))]
mod app {
    use automotive::can::bitrate::BitrateBuilder;
    use automotive::isotp::IsoTPConfig;
    use automotive::j2534::{J2534CanAdapter, J2534NativeIsoTpTransport};
    use automotive::uds::{DataIdentifier, UDSClient};
    use automotive::Result;

    use bstr::ByteSlice;
    use strum::IntoEnumIterator;

    async fn get_version(isotp: &J2534NativeIsoTpTransport, identifier: u32) -> Result<()> {
        let uds = UDSClient::new(isotp);

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

    pub async fn run() -> Result<()> {
        tracing_subscriber::fmt::init();

        let dll_path = None;
        let bitrate_cfg = BitrateBuilder::new::<J2534CanAdapter>()
            .bitrate(500_000)
            .build()
            .unwrap();

        let standard_ids = 0x700..=0x7f7;
        let extended_ids = (0x00..=0xff).map(|i| 0x18da0000 + (i << 8) + 0xf1);
        let mut ids = standard_ids.chain(extended_ids);

        let Some(first_id) = ids.next() else {
            return Ok(());
        };

        let config = IsoTPConfig::new(0, first_id.into());
        let isotp = J2534NativeIsoTpTransport::new(dll_path, bitrate_cfg, config)?;
        let _ = get_version(&isotp, first_id).await;
        let mut device = isotp.into_device();

        for id in ids {
            let config = IsoTPConfig::new(0, id.into());
            let isotp = match J2534NativeIsoTpTransport::new_on_device(device, bitrate_cfg, config)
            {
                Ok(isotp) => isotp,
                Err(_) => J2534NativeIsoTpTransport::new(dll_path, bitrate_cfg, config)?,
            };

            let _ = get_version(&isotp, id).await;
            device = isotp.into_device();
        }

        Ok(())
    }
}

#[cfg(all(target_os = "windows", feature = "j2534"))]
#[tokio::main]
async fn main() -> automotive::Result<()> {
    app::run().await
}

#[cfg(not(all(target_os = "windows", feature = "j2534")))]
fn main() {
    eprintln!("This example requires Windows and the `j2534` feature.");
}
