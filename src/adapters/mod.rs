//! Hardware adapter implementations, discovery, and shared configuration helpers.

#[cfg(all(target_os = "windows", feature = "j2534"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "windows", feature = "j2534"))))]
pub mod j2534;

#[cfg(feature = "panda")]
#[cfg_attr(docsrs, doc(cfg(feature = "panda")))]
pub mod panda;

#[cfg(feature = "peak")]
#[cfg_attr(docsrs, doc(cfg(feature = "peak")))]
pub mod peak;

#[cfg(all(target_os = "linux", feature = "socketcan"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "linux", feature = "socketcan"))))]
pub mod socketcan;

#[cfg(all(target_os = "windows", feature = "vector-xl"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "windows", feature = "vector-xl"))))]
pub mod vector;

#[cfg(any(
    feature = "panda",
    feature = "peak",
    all(target_os = "windows", feature = "vector-xl")
))]
pub(crate) fn default_bitrate(
    bitrate: crate::can::bitrate::BitrateBuilder,
) -> crate::can::bitrate::BitrateBuilder {
    bitrate
        .bitrate(500_000)
        .sample_point(0.8)
        .data_bitrate(2_000_000)
        .data_sample_point(0.8)
}

/// Default TX deadline for adapters that support cancellation.
pub const DEFAULT_TX_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[cfg(any(feature = "peak", all(target_os = "windows", feature = "vector-xl")))]
pub(crate) fn default_tx_timeout() -> Option<std::time::Duration> {
    Some(DEFAULT_TX_TIMEOUT)
}

#[cfg(any(feature = "peak", all(target_os = "windows", feature = "vector-xl")))]
pub(crate) fn validate_tx_timeout(timeout: Option<std::time::Duration>) -> crate::Result<()> {
    if timeout.is_some_and(|timeout| timeout.is_zero()) {
        return Err(crate::Error::InvalidAdapterConfig(
            "TX timeout must be nonzero".into(),
        ));
    }
    Ok(())
}

/// Convenience function to get the first available adapter on the system.
///
/// Adapter backends are feature-gated. If no adapter features are enabled, or no
/// supported hardware/interface is available, this returns [`crate::Error::NotFound`].
pub fn get_adapter() -> Result<crate::can::AsyncCanAdapter, crate::error::Error> {
    #[cfg(feature = "panda")]
    {
        let config = crate::adapters::panda::PandaConfig::builder()
            .build()
            .unwrap();
        if let Ok(panda) = crate::adapters::panda::PandaAdapter::new_async(config) {
            return Ok(panda);
        }
    }

    #[cfg(feature = "peak")]
    {
        let config = crate::adapters::peak::PeakConfig::builder()
            .build()
            .unwrap();
        if let Ok(peak) = crate::adapters::peak::PeakAdapter::new_async(config) {
            return Ok(peak);
        }
    }

    #[cfg(all(target_os = "linux", feature = "socketcan"))]
    {
        // TODO: iterate over all available SocketCAN adapters to also find things like vcan0
        for iface in ["can0", "vcan0"] {
            let config = crate::adapters::socketcan::SocketCanConfig::builder(iface).build();
            if let Ok(socket) = crate::adapters::socketcan::SocketCanAdapter::new_async(config) {
                return Ok(socket);
            }
        }
    }

    #[cfg(all(target_os = "windows", feature = "vector-xl"))]
    {
        let config = crate::adapters::vector::VectorConfig::builder(0)
            .bitrate(default_bitrate)
            .build()
            .unwrap();
        if let Ok(adapter) = crate::adapters::vector::VectorAdapter::new_async(config) {
            return Ok(adapter);
        };
    }

    #[cfg(all(target_os = "windows", feature = "j2534"))]
    {
        let bitrate_cfg =
            crate::can::bitrate::BitrateBuilder::new::<crate::adapters::j2534::J2534Adapter>()
                .bitrate(500_000)
                .build()
                .unwrap();

        if let Ok(adapter) = crate::adapters::j2534::J2534Adapter::new_async(None, bitrate_cfg) {
            return Ok(adapter);
        };
    }

    Err(crate::error::Error::NotFound)
}
