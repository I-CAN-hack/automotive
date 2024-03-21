//! This module provides a [`CanAdapter`] implementation for the [`socketcan`] crate.
use crate::can::AsyncCanAdapter;
use crate::can::CanAdapter;
use crate::error::Error;

use socketcan::socket::Socket;
use socketcan::SocketOptions;
use tracing::info;

mod frame;

/// Aadapter for a [`socketcan::CanFdSocket`].
pub struct SocketCan {
    socket: socketcan::CanFdSocket,
}

impl SocketCan {
    pub fn new(socket: socketcan::CanFdSocket) -> Self {
        socket.set_nonblocking(true).unwrap();
        socket.set_loopback(true).unwrap();
        socket.set_recv_own_msgs(true).unwrap();
        Self { socket }
    }

    pub fn new_async_from_name(name: &str) -> Result<AsyncCanAdapter, Error> {
        if let Ok(socket) = socketcan::CanFdSocket::open(name) {
            SocketCan::new_async(socket)
        } else {
            Err(crate::error::Error::NotFound)
        }
    }

    pub fn new_async(socket: socketcan::CanFdSocket) -> Result<AsyncCanAdapter, Error> {
        let socket = SocketCan::new(socket);

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
        while let Ok((frame, meta)) = self.socket.read_frame_with_meta() {
            let mut frame: crate::can::Frame = frame.into();
            frame.loopback = meta.loopback;
            frames.push(frame);
        }

        Ok(frames)
    }
}
