//! Transport layers for exchanging diagnostic messages.

pub mod doip;
pub mod isotp;

/// Abstraction over anything that can exchange diagnostic PDUs over a transport layer.
///
/// Implementations shipped with this workspace:
/// * [`isotp::IsoTP`] — software ISO-TP framing on top of any [`crate::can::CanAdapter`].
/// * [`doip::DoIP`] — diagnostic messages over IPv4 or IPv6 TCP.
/// * `J2534NativeIsoTP` — hardware ISO 15765 via a J2534 PassThru device; the
///   adapter firmware handles all framing, flow-control, and STmin timing.
pub trait Transport {
    /// Transmit a single UDS PDU. Resolves once the transport has accepted the
    /// payload for transmission (not necessarily once it has been ACKed on the bus).
    fn send<'a>(
        &'a self,
        data: &'a [u8],
    ) -> impl std::future::Future<Output = crate::Result<()>> + 'a;

    /// Infinite stream of received UDS PDUs. Each item is one complete PDU.
    ///
    /// An idle wait may yield [`crate::Error::IdleTimeout`]; polling the same
    /// stream again safely resumes waiting. Timeouts during an active transport
    /// transfer use [`crate::Error::Timeout`] and must not be treated as idle.
    fn recv(&self) -> impl crate::Stream<Item = crate::Result<Vec<u8>>> + Unpin + '_;
}
