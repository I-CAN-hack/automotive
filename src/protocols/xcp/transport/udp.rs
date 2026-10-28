//! XCP Ethernet packets over UDP, preserving datagram boundaries.
use super::{ethernet::Framing, PacketLimits, TransportFuture, XCPTransport};
use crate::Result;
use std::{collections::VecDeque, time::Duration};
use tokio::net::{ToSocketAddrs, UdpSocket};

pub struct XCPOnUDP {
    socket: UdpSocket,
    framing: Framing,
    pending: VecDeque<Vec<u8>>,
}

impl XCPOnUDP {
    pub async fn connect(address: impl ToSocketAddrs) -> Result<Self> {
        Self::connect_with_timeout(address, Duration::from_secs(1)).await
    }
    pub async fn connect_with_timeout(
        address: impl ToSocketAddrs,
        timeout: Duration,
    ) -> Result<Self> {
        tokio::time::timeout(timeout, async {
            let peer = tokio::net::lookup_host(address)
                .await?
                .next()
                .ok_or(crate::Error::NotFound)?;
            let socket = UdpSocket::bind(if peer.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })
            .await?;
            socket.connect(peer).await?;
            Ok(Self::from_socket(socket))
        })
        .await
        .map_err(|_| crate::Error::Timeout)?
    }

    /// Bind a local UDP address and port, then select the ECU peer.
    pub async fn bind(
        local_address: impl ToSocketAddrs,
        address: impl ToSocketAddrs,
    ) -> Result<Self> {
        Self::bind_with_timeout(local_address, address, Duration::from_secs(1)).await
    }

    /// Open a bound UDP transport within the given deadline.
    pub async fn bind_with_timeout(
        local_address: impl ToSocketAddrs,
        address: impl ToSocketAddrs,
        timeout: Duration,
    ) -> Result<Self> {
        tokio::time::timeout(timeout, async {
            let socket = UdpSocket::bind(local_address).await?;
            socket.connect(address).await?;
            Ok(Self::from_socket(socket))
        })
        .await
        .map_err(|_| crate::Error::Timeout)?
    }

    fn from_socket(socket: UdpSocket) -> Self {
        Self {
            socket,
            framing: Framing::new(65_503),
            pending: VecDeque::new(),
        }
    }
}

impl XCPTransport for XCPOnUDP {
    fn limits(&self) -> PacketLimits {
        self.framing.limits()
    }
    fn send<'a>(&'a mut self, data: &'a [u8]) -> TransportFuture<'a, ()> {
        Box::pin(async move {
            let frame = self.framing.encode(data)?;
            if self.socket.send(&frame).await? != frame.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "incomplete UDP datagram transmission",
                )
                .into());
            }
            Ok(())
        })
    }
    fn recv(&mut self) -> TransportFuture<'_, Vec<u8>> {
        Box::pin(async move {
            if self.pending.is_empty() {
                let mut bytes = vec![0; 65_536];
                let size = self.socket.recv(&mut bytes).await?;
                bytes.truncate(size);
                let mut packets = VecDeque::new();
                while let Some(packet) = self.framing.decode(&mut bytes, true)? {
                    packets.push_back(packet);
                }
                if packets.is_empty() {
                    return Err(crate::Error::MalformedFrame);
                }
                self.pending = packets;
            }
            Ok(self.pending.pop_front().unwrap())
        })
    }
    fn reset(&mut self) -> Result<()> {
        self.framing.reset(&[])
    }
}
