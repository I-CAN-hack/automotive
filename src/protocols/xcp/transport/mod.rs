//! XCP transport layers for CAN, CAN FD, TCP and UDP.

mod can;
mod ethernet;
mod tcp;
mod udp;

pub use can::{XCPCanConfig, XCPOnCAN};
pub use ethernet::Error;
pub use tcp::XCPOnTCP;
pub use udp::XCPOnUDP;

use std::{future::Future, pin::Pin};

/// An asynchronous XCP transport operation.
pub type TransportFuture<'a, T> = Pin<Box<dyn Future<Output = crate::Result<T>> + Send + 'a>>;

/// Packet limits exclude transport headers and trailers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketLimits {
    pub max_tx_packet_size: usize,
    pub max_rx_packet_size: usize,
    /// The carrier may append fill bytes to a received packet, as CAN does.
    pub padded: bool,
}

/// An exclusive XCP packet transport, including carrier-specific framing.
///
/// A client may serialize several send/receive exchanges as one operation.
/// Receive implementations retain partial framing state across cancellation.
pub trait XCPTransport: Send {
    fn limits(&self) -> PacketLimits;
    fn send<'a>(&'a mut self, data: &'a [u8]) -> TransportFuture<'a, ()>;
    fn recv(&mut self) -> TransportFuture<'_, Vec<u8>>;

    /// Prepare command recovery, retaining pending bytes/packets. A transport
    /// whose packet boundaries are uncertain can require a fresh connection.
    fn reset(&mut self) -> crate::Result<()> {
        Ok(())
    }
}

impl<T: XCPTransport + ?Sized> XCPTransport for Box<T> {
    fn limits(&self) -> PacketLimits {
        (**self).limits()
    }
    fn send<'a>(&'a mut self, data: &'a [u8]) -> TransportFuture<'a, ()> {
        (**self).send(data)
    }
    fn recv(&mut self) -> TransportFuture<'_, Vec<u8>> {
        (**self).recv()
    }
    fn reset(&mut self) -> crate::Result<()> {
        (**self).reset()
    }
}
