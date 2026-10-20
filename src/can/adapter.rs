//! Convenience functions to get a CAN adapter.

/// Convenience function to get the first available adapter on the system.
///
/// Adapter backends are feature-gated. If no adapter features are enabled, or no
/// supported hardware/interface is available, this returns [`crate::error::Error::NotFound`].
pub fn get_adapter() -> Result<crate::can::AsyncCanAdapter, crate::error::Error> {
    #[cfg(feature = "panda")]
    {
        let config = crate::panda::PandaConfig::builder()
            .bitrate(|bitrate| {
                bitrate
                    .bitrate(500_000)
                    .sample_point(0.8)
                    .data_bitrate(2_000_000)
                    .data_sample_point(0.8)
            })
            .build()
            .unwrap();
        if let Ok(panda) = crate::panda::Panda::new_async(config) {
            return Ok(panda);
        }
    }

    #[cfg(feature = "peak")]
    {
        let config = crate::peak::PeakConfig::builder()
            .bitrate(|bitrate| {
                bitrate
                    .bitrate(500_000)
                    .sample_point(0.8)
                    .data_bitrate(2_000_000)
                    .data_sample_point(0.8)
            })
            .build()
            .unwrap();
        if let Ok(peak) = crate::peak::Peak::new_async(config) {
            return Ok(peak);
        }
    }

    #[cfg(all(target_os = "linux", feature = "socketcan"))]
    {
        // TODO: iterate over all available SocketCAN adapters to also find things like vcan0
        for iface in ["can0", "vcan0"] {
            let config = crate::socketcan::SocketCanConfig::builder(iface).build();
            if let Ok(socket) = crate::socketcan::SocketCan::new_async(config) {
                return Ok(socket);
            }
        }
    }

    #[cfg(all(target_os = "windows", feature = "vector-xl"))]
    {
        let config = crate::vector::VectorConfig::builder(0)
            .bitrate(|bitrate| {
                bitrate
                    .bitrate(500_000)
                    .sample_point(0.8)
                    .data_bitrate(2_000_000)
                    .data_sample_point(0.8)
            })
            .build()
            .unwrap();
        if let Ok(adapter) = crate::vector::VectorCan::new_async(config) {
            return Ok(adapter);
        };
    }

    #[cfg(all(target_os = "windows", feature = "j2534"))]
    {
        let bitrate_cfg =
            crate::can::bitrate::BitrateBuilder::new::<crate::j2534::J2534CanAdapter>()
                .bitrate(500_000)
                .build()
                .unwrap();

        if let Ok(adapter) = crate::j2534::J2534CanAdapter::new_async(None, bitrate_cfg) {
            return Ok(adapter);
        };
    }

    Err(crate::error::Error::NotFound)
}
