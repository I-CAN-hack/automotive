//! This module provides a [`CanAdapter`] implementation for the [`socketcan`] crate.
use crate::can::AsyncCanAdapter;
use crate::can::CanAdapter;
use crate::Result;

use socketcan::socket::Socket;
use socketcan::SocketOptions;
use std::collections::VecDeque;

mod frame;

const IFF_ECHO: u64 = 1 << 18; // include/uapi/linux/if.h

/// Aadapter for a [`socketcan::CanFdSocket`].
pub struct SocketCan {
    socket: socketcan::CanFdSocket,

    /// If the IFF_ECHO flag is set on the interface, it will implement proper ACK logic. If the flag is not set, the kernel will emulate this.
    #[allow(dead_code)]
    iff_echo: bool,
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
        let socket = match socketcan::CanFdSocket::open(name) {
            Ok(socket) => socket,
            Err(_) => return Err(crate::error::Error::NotFound),
        };

        socket.set_nonblocking(true).unwrap();
        socket.set_loopback(true).unwrap();
        socket.set_recv_own_msgs(true).unwrap();

        // Attempt to increase the buffer receive size to 1MB
        socket.as_raw_socket().set_recv_buffer_size(1_000_000).ok();

        if let Ok(sz) = socket.as_raw_socket().recv_buffer_size() {
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

        if !iff_echo {
            tracing::warn!("IFF_ECHO is not set on the interface. ACK support is emulated by the Linux Kernel.");
        }

        Ok(SocketCan { socket, iff_echo })
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
