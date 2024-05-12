//! Low Level SocketCAN code
//! Code based on socketcan-rs
use libc::{
    c_int, c_void, can_frame, canfd_frame, sa_family_t, sockaddr_can, socklen_t, AF_CAN, CANFD_MTU,
    CAN_MTU, CAN_RAW, CAN_RAW_FD_FRAMES, CAN_RAW_LOOPBACK, CAN_RAW_RECV_OWN_MSGS, SOL_CAN_RAW,
};
use std::io::Write;
use std::os::fd::AsRawFd;

use crate::can::Frame;
use crate::socketcan::frame::{can_frame_default, canfd_frame_default};

pub struct CanFdSocket(socket2::Socket);

fn if_nametoindex(name: &str) -> std::io::Result<libc::c_uint> {
    let c_name = std::ffi::CString::new(name).unwrap();
    let if_index = unsafe { libc::if_nametoindex(c_name.as_ptr()) };

    if if_index == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(if_index)
    }
}

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

    pub fn write_frame(&self, frame: &Frame) -> std::io::Result<()> {
        match frame.fd {
            true => {
                let frame = canfd_frame::from(frame);
                let bytes = as_bytes(&frame);
                self.as_raw_socket().write_all(bytes)
            }
            false => {
                let frame = can_frame::from(frame);
                let bytes = as_bytes(&frame);
                self.as_raw_socket().write_all(bytes)
            }
        }
    }

    pub fn read_frame(&self) -> std::io::Result<Frame> {
        let mut frame = Vec::with_capacity(CANFD_MTU);

        let buf = socket2::MaybeUninitSlice::new(frame.spare_capacity_mut());
        let buf_slice = &mut [buf];

        let mut header = socket2::MsgHdrMut::new().with_buffers(buf_slice);

        match self.as_raw_socket().recvmsg(&mut header, 0)? {
            // If we only get 'can_frame' number of bytes, then the return is,
            // by definition, a can_frame, so we just copy the bytes into the
            // proper type.
            CAN_MTU => {
                let loopback = header.flags().is_confirm();

                // SAFETY: just received CAN_MTU bytes
                unsafe {
                    frame.set_len(CAN_MTU);
                }

                let mut ret = can_frame_default();
                as_bytes_mut(&mut ret).copy_from_slice(&frame);

                let mut frame = Frame::from(ret);
                frame.loopback = loopback;
                Ok(frame)
            }
            CANFD_MTU => {
                let loopback = header.flags().is_confirm();

                // SAFETY: just received CANFD_MTU bytes
                unsafe {
                    frame.set_len(CANFD_MTU);
                }

                let mut ret = canfd_frame_default();
                as_bytes_mut(&mut ret).copy_from_slice(&frame);

                let mut frame = Frame::from(ret);
                frame.fd = true;
                frame.loopback = loopback;
                Ok(frame)
            }
            _ => Err(std::io::Error::last_os_error()),
        }
    }

    pub fn set_fd_mode(&self, enabled: bool) -> std::io::Result<()> {
        let enable = c_int::from(enabled);
        self.set_socket_option(SOL_CAN_RAW, CAN_RAW_FD_FRAMES, &enable)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        self.as_raw_socket().set_nonblocking(nonblocking)
    }

    /// Enable or disable loopback.
    ///
    /// By default, loopback is enabled, causing other applications that open
    /// the same CAN bus to see frames emitted by different applications on
    /// the same system.
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

    /// Enable or disable receiving of own frames.
    ///
    /// When enabled, this settings controls if CAN frames sent
    /// are received back by sender when ACKed. Default is off.
    pub fn set_recv_own_msgs(&self, enabled: bool) -> std::io::Result<()> {
        let recv_own_msgs = c_int::from(enabled);
        self.set_socket_option(SOL_CAN_RAW, CAN_RAW_RECV_OWN_MSGS, &recv_own_msgs)
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
