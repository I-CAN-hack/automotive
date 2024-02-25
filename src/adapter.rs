//! Convenience functions to get a CAN adapter.

#[cfg(target_os = "linux")]
use socketcan::Socket;

/// Convenience function to get the first available adapter on the system. Supports both comma.ai panda, and SocketCAN.
pub fn get_adapter() -> Result<crate::async_can::AsyncCanAdapter, crate::error::Error> {
    if let Ok(panda) = crate::panda::Panda::new_async() {
        return Ok(panda);
    }

    #[cfg(target_os = "linux")]
    {
        // TODO: iterate over all available SocketCAN adapters to also find things like vcan0
        if let Ok(socket) = socketcan::CanFdSocket::open("can0") {
            return Ok(crate::socketcan::SocketCan::new_async(socket).unwrap());
        }
    }

    Err(crate::error::Error::NotFound)
}
