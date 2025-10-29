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
        if let Ok(adapter) =
            crate::vector::VectorCan::new_async(0, &Some(crate::vector::CONFIG_500K_2M_80))
        {
            return Ok(adapter);
        };
    }

    Err(crate::error::Error::NotFound)
}
