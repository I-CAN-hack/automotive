//! Convenience functions to get a CAN adapter.

/// Convenience function to get the first available adapter on the system. Supports both comma.ai panda, and SocketCAN.
pub fn get_adapter() -> Result<crate::can::AsyncCanAdapter, crate::error::Error> {
    if let Ok(panda) = crate::panda::Panda::new_async() {
        return Ok(panda);
    }

    #[cfg(target_os = "linux")]
    {
        // TODO: iterate over all available SocketCAN adapters to also find things like vcan0
        if let Ok(socket) = crate::socketcan::SocketCan::new_async("can0") {
            return Ok(socket);
        }
    }

    Err(crate::error::Error::NotFound)
}
