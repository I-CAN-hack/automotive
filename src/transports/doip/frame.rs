//! ISO 13400-2:2025 §9.2: eight-byte header and network byte order.
use super::Error;
use crate::Result;

pub(super) const HEADER_NACK: u16 = 0;
pub(super) const ANNOUNCEMENT: u16 = 4;
pub(super) const ACTIVATION_REQUEST: u16 = 5;
pub(super) const ACTIVATION_RESPONSE: u16 = 6;
pub(super) const ALIVE_REQUEST: u16 = 7;
pub(super) const ALIVE_RESPONSE: u16 = 8;
pub(super) const DIAGNOSTIC: u16 = 0x8001;
pub(super) const ACK: u16 = 0x8002;
pub(super) const NACK: u16 = 0x8003;

pub(super) fn invalid(reason: &str) -> crate::Error {
    Error::InvalidMessage(reason.into()).into()
}

pub(super) fn encode(version: u8, kind: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![version, !version];
    bytes.extend(kind.to_be_bytes());
    bytes.extend((payload.len() as u32).to_be_bytes());
    bytes.extend(payload);
    bytes
}

pub(super) struct Message {
    pub version: u8,
    pub kind: u16,
    pub payload: Vec<u8>,
}

/// Check lengths at the header, before buffering or allocating a claimed payload.
pub(super) fn header(bytes: &[u8], max_size: usize) -> Result<(u8, u16, usize)> {
    if bytes.len() < 8 || !(2..=4).contains(&bytes[0]) || bytes[1] != !bytes[0] {
        return Err(invalid("invalid protocol version or inverse version"));
    }
    let kind = u16::from_be_bytes([bytes[2], bytes[3]]);
    let size = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if size > max_size {
        return Err(Error::PayloadTooLarge.into());
    }
    let valid = match kind {
        HEADER_NACK => size == 1,
        ANNOUNCEMENT => matches!(size, 32 | 33),
        ACTIVATION_RESPONSE => matches!(size, 9 | 13),
        ALIVE_REQUEST => size == 0,
        DIAGNOSTIC | ACK | NACK => size >= 5,
        _ => true, // Unknown payloads are discarded by the client, without a NACK.
    };
    if !valid {
        return Err(invalid("invalid payload length for message type"));
    }
    Ok((bytes[0], kind, size))
}

pub(super) fn decode(bytes: &mut Vec<u8>, max_size: usize) -> Result<Option<Message>> {
    if bytes.len() < 8 {
        return Ok(None);
    }
    let (version, kind, size) = header(bytes, max_size)?;
    if bytes.len() - 8 < size {
        return Ok(None);
    }
    let payload = bytes[8..8 + size].to_vec();
    bytes.drain(..8 + size);
    Ok(Some(Message {
        version,
        kind,
        payload,
    }))
}
