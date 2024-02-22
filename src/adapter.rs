
#[cfg(target_os = "linux")]
use socketcan::Socket;


pub fn get_adapter() -> Result<crate::async_can::AsyncCanAdapter, crate::error::Error> {
    if let Ok(panda) = crate::panda::Panda::new() {
        return Ok(panda);
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(socket) = socketcan::CanFdSocket::open("can0") {
            return Ok(crate::socketcan::SocketCan::new(socket).unwrap())
        }

        if let Ok(socket) = socketcan::CanSocket::open("can0") {
            return Ok(crate::socketcan::SocketCan::new(socket).unwrap())
        }
    }

    Err(crate::error::Error::NotFound)
}
