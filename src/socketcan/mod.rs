//! This module provides a [`CanAdapter`] implementation for the [`socketcan`] crate.
use crate::can::AsyncCanAdapter;
use crate::can::CanAdapter;
use crate::Result;

use socketcan::socket::Socket;
use socketcan::SocketOptions;
use tracing::info;
use std::collections::VecDeque;

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

    pub fn new_async_from_name(name: &str) -> Result<AsyncCanAdapter> {
        if let Ok(socket) = socketcan::CanFdSocket::open(name) {
            SocketCan::new_async(socket)
        } else {
            Err(crate::error::Error::NotFound)
        }
    }

    pub fn new_async(socket: socketcan::CanFdSocket) -> Result<AsyncCanAdapter> {
        let socket = SocketCan::new(socket);

        info!("Connected to SocketCan");
        Ok(AsyncCanAdapter::new(socket))
    }
}

impl CanAdapter for SocketCan {
    fn send(&mut self, frames: &mut VecDeque<crate::can::Frame>) -> Result<()> {
        while let Some(frame) = frames.pop_front() {
            let to_send: socketcan::frame::CanAnyFrame = frame.clone().into();

            if let Err(_) = self.socket.write_frame(&to_send) {
                // Failed to send frame, push it back to the front of the queue for next iteration
                frames.push_front(frame);
                break;
            }
        }

        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<crate::can::Frame>> {
        let mut frames = vec![];
        while let Ok((frame, meta)) = self.socket.read_frame_with_meta() {
            let mut frame: crate::can::Frame = frame.into();
            frame.loopback = meta.loopback;
            frames.push(frame);
        }

        Ok(frames)
    }
}
