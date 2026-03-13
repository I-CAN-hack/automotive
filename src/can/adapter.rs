//! Convenience functions to get a CAN adapter.

/// Convenience function to get the first available adapter on the system. Supports both comma.ai panda, and SocketCAN.
pub fn get_adapter() -> Result<crate::can::AsyncCanAdapter, crate::error::Error> {
    #[cfg(feature = "panda")]
    {
        if let Ok(panda) = crate::panda::Panda::new_async() {
            return Ok(panda);
        }
    }

    #[cfg(all(target_os = "linux", feature = "socketcan"))]
    {
        // TODO: iterate over all available SocketCAN adapters to also find things like vcan0
        for iface in ["can0", "vcan0"] {
            if let Ok(socket) = crate::socketcan::SocketCan::new_async(iface) {
                return Ok(socket);
            }
        }
    }

    #[cfg(all(target_os = "windows", feature = "vector-xl"))]
    {
        let bitrate_cfg = crate::can::bitrate::BitrateBuilder::new::<crate::vector::VectorCan>()
            .bitrate(500_000)
            .sample_point(0.8)
            .sjw(1)
            .data_bitrate(2_000_000)
            .data_sample_point(0.8)
            .data_sjw(1)
            .build()
            .unwrap();

        if let Ok(adapter) = crate::vector::VectorCan::new_async(0, Some(bitrate_cfg)) {
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
