//! This module provides a [`CanAdapter`] implementation for the [`socketcan`] crate.
use crate::async_can::AsyncCanAdapter;
use crate::can::CanAdapter;
use crate::error::Error;

use socketcan::socket::Socket;
use tracing::info;

mod frame;

/// Aadapter for a [`socketcan::CanFdSocket`].
pub struct SocketCan {
    socket: socketcan::CanFdSocket,
}

impl SocketCan {
    pub fn new_async(socket: socketcan::CanFdSocket) -> Result<AsyncCanAdapter, Error> {
        socket.set_nonblocking(true).unwrap();
        let socket = SocketCan { socket };

        info!("Connected to SocketCan");
        Ok(AsyncCanAdapter::new(socket))
    }
}

impl CanAdapter for SocketCan {
    fn send(&mut self, frames: &[crate::can::Frame]) -> Result<(), Error> {
        for frame in frames {
            let frame: socketcan::frame::CanAnyFrame = frame.clone().into();
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
