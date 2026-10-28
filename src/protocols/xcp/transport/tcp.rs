//! XCP Ethernet packets over a TCP byte stream.
use super::{ethernet::Framing, Error, PacketLimits, TransportFuture, XCPTransport};
use crate::Result;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, ToSocketAddrs},
};

pub struct XCPOnTCP {
    stream: TcpStream,
    framing: Framing,
    pending: Vec<u8>,
    writing: bool,
}

impl XCPOnTCP {
    pub async fn connect(address: impl ToSocketAddrs) -> Result<Self> {
        Self::connect_with_timeout(address, Duration::from_secs(1)).await
    }
    pub async fn connect_with_timeout(
        address: impl ToSocketAddrs,
        timeout: Duration,
    ) -> Result<Self> {
        let stream = tokio::time::timeout(timeout, TcpStream::connect(address))
            .await
            .map_err(|_| crate::Error::Timeout)??;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            framing: Framing::new(u16::MAX as usize),
            pending: Vec::new(),
            writing: false,
        })
    }
}

impl XCPTransport for XCPOnTCP {
    fn limits(&self) -> PacketLimits {
        self.framing.limits()
    }
    fn send<'a>(&'a mut self, data: &'a [u8]) -> TransportFuture<'a, ()> {
        Box::pin(async move {
            let frame = self.framing.encode(data)?;
            self.writing = true;
            self.stream.write_all(&frame).await?;
            self.writing = false;
            Ok(())
        })
    }
    fn recv(&mut self) -> TransportFuture<'_, Vec<u8>> {
        Box::pin(async move {
            loop {
                if let Some(packet) = self.framing.decode(&mut self.pending, false)? {
                    return Ok(packet);
                }
                if self.stream.read_buf(&mut self.pending).await? == 0 {
                    return Err(crate::Error::Disconnected);
                }
            }
        })
    }
    fn reset(&mut self) -> Result<()> {
        if self.writing {
            return Err(Error::ReconnectRequired.into());
        }
        self.framing.reset(&self.pending)
    }
}
