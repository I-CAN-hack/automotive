//! XCP Ethernet LEN/CTR framing shared by the TCP and UDP implementations.
use super::PacketLimits;
use crate::Result;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error("Malformed XCP Ethernet packet: {reason}; bytes: {response:02x?}")]
    InvalidPacket { reason: String, response: Vec<u8> },
    #[error("XCP Ethernet counter mismatch: expected {expected}, received {actual}")]
    CounterMismatch { expected: u16, actual: u16 },
    #[error("XCP TCP packet boundaries are uncertain; reopen the transport")]
    ReconnectRequired,
}

fn malformed(reason: &str, response: &[u8]) -> crate::Error {
    Error::InvalidPacket {
        reason: reason.into(),
        response: response.to_vec(),
    }
    .into()
}

/// ASAM XCP Ethernet 1.1 section 1.3: little-endian LEN/CTR and independent counters.
pub(super) struct Framing {
    max_packet_size: usize,
    tx_counter: u16,
    rx_counter: Option<u16>,
}
impl Framing {
    pub(super) fn new(max_packet_size: usize) -> Self {
        Self {
            max_packet_size,
            tx_counter: 0,
            rx_counter: None,
        }
    }
    pub(super) fn limits(&self) -> PacketLimits {
        PacketLimits {
            max_tx_packet_size: self.max_packet_size,
            max_rx_packet_size: self.max_packet_size,
            padded: false,
        }
    }
    pub(super) fn encode(&mut self, packet: &[u8]) -> Result<Vec<u8>> {
        if packet.is_empty() || packet.len() > self.max_packet_size {
            return Err(malformed("invalid packet length", &[]));
        }
        let mut frame = (packet.len() as u16).to_le_bytes().to_vec();
        frame.extend(self.tx_counter.to_le_bytes());
        frame.extend(packet);
        self.tx_counter = self.tx_counter.wrapping_add(1);
        Ok(frame)
    }
    pub(super) fn decode(
        &mut self,
        bytes: &mut Vec<u8>,
        complete: bool,
    ) -> Result<Option<Vec<u8>>> {
        if bytes.is_empty() {
            return Ok(None);
        }
        if bytes.len() < 4 {
            return if complete {
                Err(malformed("truncated UDP XCP header", bytes))
            } else {
                Ok(None)
            };
        }
        let size = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        if size == 0 || size > self.max_packet_size {
            return Err(malformed("invalid Ethernet packet length", bytes));
        }
        if bytes.len() < size + 4 {
            return if complete {
                Err(malformed("XCP packet crosses UDP datagram boundary", bytes))
            } else {
                Ok(None)
            };
        }
        let actual = u16::from_le_bytes([bytes[2], bytes[3]]);
        let packet = bytes[4..size + 4].to_vec();
        bytes.drain(..size + 4);
        if let Some(expected) = self
            .rx_counter
            .replace(actual.wrapping_add(1))
            .filter(|e| *e != actual)
        {
            return Err(Error::CounterMismatch { expected, actual }.into());
        }
        Ok(Some(packet))
    }
    pub(super) fn reset(&mut self, pending: &[u8]) -> Result<()> {
        if pending.len() >= 4 && pending[..2] == [0, 0] {
            return Err(Error::ReconnectRequired.into());
        }
        self.rx_counter = None;
        Ok(())
    }
}
