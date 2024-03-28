//! This module provides a [`CanAdapter`] implementation for the [`socketcan`] crate.
use crate::can::AsyncCanAdapter;
use crate::can::CanAdapter;
use crate::Result;

use socketcan::socket::Socket;
use socketcan::SocketOptions;
use std::collections::VecDeque;
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

        // Attempt to increase the buffer receive size to 1MB
        socket.as_raw_socket().set_recv_buffer_size(1_000_000).ok();

        if let Ok(sz) = socket.as_raw_socket().recv_buffer_size() {
            info!("SocketCAN receive buffer size {}", sz);
        }

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

            if self.socket.write_frame(&to_send).is_err() {
                // Failed to send frame, push it back to the front of the queue for next send call
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
