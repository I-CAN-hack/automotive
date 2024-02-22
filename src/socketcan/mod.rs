use crate::async_can::AsyncCanAdapter;
use crate::can::CanAdapter;
use crate::error::Error;

use socketcan::frame::AsPtr;
use socketcan::socket::Socket;

pub mod frame;

pub struct SocketCan<T: Socket + Send + Sync + 'static> {
    socket: T,
}

impl<T> SocketCan<T>
where
    T: Socket + Send + Sync + 'static,
    crate::can::Frame: From<<T as Socket>::FrameType>,
    <T as Socket>::FrameType: From<crate::can::Frame>,
    <T as Socket>::FrameType: AsPtr,
{
    pub fn new(socket: T) -> Result<AsyncCanAdapter, Error> {
        socket.set_nonblocking(true).unwrap();
        let socket = SocketCan { socket };
        Ok(AsyncCanAdapter::new(socket))
    }
}

impl<T> CanAdapter for SocketCan<T>
where
    T: Socket + Send + Sync + 'static,
    crate::can::Frame: From<<T as Socket>::FrameType>,
    <T as Socket>::FrameType: From<crate::can::Frame>,
    <T as Socket>::FrameType: AsPtr,
{
    fn send(&mut self, frames: &[crate::can::Frame]) -> Result<(), Error> {
        for frame in frames {
            let frame: <T as Socket>::FrameType = frame.clone().into();
            self.socket.write_frame(&frame).unwrap();
        }

        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<crate::can::Frame>, Error> {
        let mut frames = vec![];
        while let Ok(frame) = self.socket.read_frame() {
            frames.push(frame.into());
        }

        Ok(frames)
    }
}
