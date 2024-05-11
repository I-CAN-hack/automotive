//! Low Level SocketCAN code
//! Code based on https://github.com/socketcan-rs/socketcan-rs
use libc::{
    c_int, c_void, sa_family_t, sockaddr_can, socklen_t, AF_CAN, CAN_RAW, CAN_RAW_LOOPBACK, PF_CAN,
    SOL_CAN_RAW,
};
use nix::net::if_::if_nametoindex;
use std::os::fd::AsRawFd;
use tokio::io;

pub struct CanFdSocket(socket2::Socket);

fn as_bytes<T: Sized>(val: &T) -> &[u8] {
    let sz = std::mem::size_of::<T>();
    unsafe { std::slice::from_raw_parts::<'_, u8>(val as *const _ as *const u8, sz) }
}

fn as_bytes_mut<T: Sized>(val: &mut T) -> &mut [u8] {
    let sz = std::mem::size_of::<T>();
    unsafe { std::slice::from_raw_parts_mut(val as *mut _ as *mut u8, sz) }
}

impl CanFdSocket {
    pub fn open(ifname: &str) -> std::io::Result<Self> {
        let mut addr: sockaddr_can = unsafe { std::mem::zeroed() };
        addr.can_family = AF_CAN as sa_family_t;
        addr.can_ifindex = if_nametoindex(ifname)? as c_int;

        // Convert into sockaddr_storage
        let bytes = as_bytes(&addr);
        let len = bytes.len();
        let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        as_bytes_mut(&mut storage)[..len].copy_from_slice(bytes);
        let sock_addr = unsafe { socket2::SockAddr::new(storage, len as socklen_t) };

        let af_can = socket2::Domain::from(AF_CAN);
        let can_raw = socket2::Protocol::from(CAN_RAW);

        let sock = socket2::Socket::new_raw(af_can, socket2::Type::RAW, Some(can_raw))?;
        sock.bind(&sock_addr)?;
        Ok(Self(sock))
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        self.as_raw_socket().set_nonblocking(nonblocking)
    }

    pub fn set_loopback(&self, enabled: bool) -> std::io::Result<()> {
        let loopback = c_int::from(enabled);
        self.set_socket_option(SOL_CAN_RAW, CAN_RAW_LOOPBACK, &loopback)
    }

    pub fn set_recv_buffer_size(&self, size: usize) -> std::io::Result<()> {
        self.as_raw_socket().set_recv_buffer_size(size)
    }

    pub fn recv_buffer_size(&self) -> std::io::Result<usize> {
        self.as_raw_socket().recv_buffer_size()
    }

    fn as_raw_socket(&self) -> &socket2::Socket {
        &self.0
    }

    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.0.as_raw_fd()
    }

    fn set_socket_option<T>(&self, level: c_int, name: c_int, val: &T) -> std::io::Result<()> {
        let ret = unsafe {
            libc::setsockopt(
                self.as_raw_fd(),
                level,
                name,
                val as *const _ as *const c_void,
                std::mem::size_of::<T>() as socklen_t,
            )
        };

        match ret {
            0 => Ok(()),
            _ => Err(std::io::Error::last_os_error()),
        }
    }
}
