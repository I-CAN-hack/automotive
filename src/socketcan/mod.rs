//! This module provides a [`CanAdapter`] implementation for the [`socketcan`] crate.
use crate::can::{AsyncCanAdapter, CanAdapter, Frame};
use crate::socketcan::socket::CanFdSocket;
use crate::Result;

use std::collections::VecDeque;

mod frame;
mod socket;

const IFF_ECHO: u64 = 1 << 18; // include/uapi/linux/if.h

/// Aadapter for a [`socketcan::CanFdSocket`].
pub struct SocketCan {
    socket: CanFdSocket,
    /// If the IFF_ECHO flag is set on the interface, it will implement proper ACK logic.
    iff_echo: bool,
    /// Queue used for fake loopback frames if IFF_ECHO is not set.
    loopback_queue: VecDeque<Frame>,
}

fn read_iff_echo(if_name: &str) -> Option<bool> {
    // Check IFF_ECHO. SIOCGIFADDR only returns the lower 16 bits of the interface flags,
    // so we read the value from sysfs instead. Alternatively, we could use netlink, but
    // that is probably not worth the complexity.

    let flags = std::fs::read_to_string(format!("/sys/class/net/{}/flags", if_name)).ok()?;
    let flags = flags.trim();
    let flags = flags.strip_prefix("0x")?;
    let flags = u64::from_str_radix(flags, 16).ok()?;

    Some(flags & IFF_ECHO != 0)
}

impl SocketCan {
    pub fn new_async(name: &str) -> Result<AsyncCanAdapter> {
        let socket = SocketCan::new(name)?;
        Ok(AsyncCanAdapter::new(socket))
    }

    pub fn new(name: &str) -> Result<SocketCan> {
        let socket = match CanFdSocket::open(name) {
            Ok(socket) => socket,
            Err(_) => return Err(crate::error::Error::NotFound),
        };

        socket.set_fd_mode(true).unwrap();
        socket.set_nonblocking(true).unwrap();
        socket.set_loopback(true).unwrap();

        // Attempt to increase the buffer receive size to 1MB
        socket.set_recv_buffer_size(1_000_000).ok();

        if let Ok(sz) = socket.recv_buffer_size() {
            tracing::info!("SocketCAN receive buffer size {}", sz);
        }

        // Read IFF_ECHO flag from interface
        let iff_echo = match read_iff_echo(name) {
            Some(iff_echo) => iff_echo,
            None => {
                tracing::warn!("Could not read flags for interface. Assuming IFF_ECHO is not set.");
                false
            }
        };

        if iff_echo {
            // socket.set_recv_own_msgs(true).unwrap();
        } else {
            tracing::warn!("IFF_ECHO is not set on the interface. ACK support is emulated.");
        }

        Ok(SocketCan {
            socket,
            iff_echo,
            loopback_queue: VecDeque::new(),
        })
    }
}

impl CanAdapter for SocketCan {
    fn send(&mut self, frames: &mut VecDeque<Frame>) -> Result<()> {
        while let Some(frame) = frames.pop_front() {
            if self.socket.write_frame(frame.clone()).is_err() {
                // Failed to send frame, push it back to the front of the queue for next send call
                frames.push_front(frame);
                break;
            } else if !self.iff_echo {
                // If IFF_ECHO is not set, we need to emulate the ACK logic.
                let mut frame = frame.clone();
                frame.loopback = true;
                self.loopback_queue.push_back(frame);
            }
        }

        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<Frame>> {
        let mut frames = vec![];

        loop {
            match self.socket.read_frame() {
                Ok(frame) => {
                    frames.push(frame);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(e) => {
                    tracing::error!("Error reading frame: {}", e);
                    return Err(crate::error::Error::Disconnected);
                }
            }
        }

        // Add fake loopback frames to the receive queue
        frames.extend(self.loopback_queue.drain(..));

        Ok(frames)
    }
}
