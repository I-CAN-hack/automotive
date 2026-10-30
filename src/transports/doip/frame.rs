//! DoIP headers and message payloads (ISO 13400-2:2025).

use super::Error;
use crate::Result;

const HEADER_LEN: usize = 8;

// ISO 13400-2:2025 §9.2, Table 17: payload type values.
const HEADER_NACK: u16 = 0;
const IDENTIFICATION: u16 = 1;
const IDENTIFICATION_EID: u16 = 2;
const IDENTIFICATION_VIN: u16 = 3;
const ANNOUNCEMENT: u16 = 4;
const ACTIVATION_REQUEST: u16 = 5;
const ACTIVATION_RESPONSE: u16 = 6;
const ALIVE_REQUEST: u16 = 7;
const ALIVE_RESPONSE: u16 = 8;
const DIAGNOSTIC: u16 = 0x8001;
const ACK: u16 = 0x8002;
const NACK: u16 = 0x8003;

pub(super) fn invalid(reason: &str) -> crate::Error {
    Error::InvalidMessage(reason.into()).into()
}

/// Logical addresses select diagnostic applications, independently of IP addresses.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct LogicalAddresses {
    pub source: u16,
    pub target: u16,
}

impl LogicalAddresses {
    fn read(reader: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            source: reader.read_u16()?,
            target: reader.read_u16()?,
        })
    }

    fn encode(self, bytes: &mut Vec<u8>) {
        bytes.extend(self.source.to_be_bytes());
        bytes.extend(self.target.to_be_bytes());
    }
}

/// Client-to-vehicle messages. Their layouts are shared by TCP and UDP encoding.
pub(super) enum Request<'a> {
    VehicleIdentification,
    VehicleIdentificationEid([u8; 6]),
    VehicleIdentificationVin([u8; 17]),
    RoutingActivation {
        source_address: u16,
        activation_type: u8,
        oem: Option<[u8; 4]>,
    },
    AliveCheckResponse {
        source_address: u16,
    },
    Diagnostic {
        addresses: LogicalAddresses,
        data: &'a [u8],
    },
}

impl Request<'_> {
    pub fn encode(self, version: u8) -> Vec<u8> {
        let mut payload = Vec::new();
        let kind = match self {
            // §7.4, Tables 2–4: unfiltered, EID-filtered, and VIN-filtered requests.
            Self::VehicleIdentification => IDENTIFICATION,
            Self::VehicleIdentificationEid(eid) => {
                payload.extend(eid);
                IDENTIFICATION_EID
            }
            Self::VehicleIdentificationVin(vin) => {
                payload.extend(vin);
                IDENTIFICATION_VIN
            }
            // §12.5, Table 53: tester address, activation type, reserved zeroes,
            // and an optional four-byte manufacturer-specific field.
            Self::RoutingActivation {
                source_address,
                activation_type,
                oem,
            } => {
                payload.extend(source_address.to_be_bytes());
                payload.push(activation_type);
                payload.extend([0; 4]);
                if let Some(oem) = oem {
                    payload.extend(oem);
                }
                ACTIVATION_REQUEST
            }
            // §9.6, Table 28: an alive response contains only the tester address.
            Self::AliveCheckResponse { source_address } => {
                payload.extend(source_address.to_be_bytes());
                ALIVE_RESPONSE
            }
            // §9.5, Table 21: logical source and target precede the diagnostic PDU.
            Self::Diagnostic { addresses, data } => {
                addresses.encode(&mut payload);
                payload.extend(data);
                DIAGNOSTIC
            }
        };

        // §9.2, Table 16 and §7.2: eight-byte header, inverse version, and
        // big-endian fields. The length excludes the generic header itself.
        let mut bytes = vec![version, !version];
        bytes.extend(kind.to_be_bytes());
        bytes.extend((payload.len() as u32).to_be_bytes());
        bytes.extend(payload);
        bytes
    }
}

/// A decoded DoIP message has a protocol version and a payload selected by its type.
pub(super) struct Message {
    pub version: u8,
    pub payload: Payload,
}

pub(super) enum Payload {
    HeaderNack(u8),
    Announcement(Announcement),
    RoutingActivation {
        tester_address: u16,
        entity_address: u16,
        code: u8,
    },
    AliveCheckRequest,
    Diagnostic {
        addresses: LogicalAddresses,
        data: Vec<u8>,
    },
    DiagnosticAcknowledgement {
        addresses: LogicalAddresses,
        code: AcknowledgementCode,
        previous_data: Vec<u8>,
    },
    /// Unsupported payload types are ignored after header and length validation.
    Other,
}

/// A DoIP acknowledgement confirms or rejects routing of a diagnostic request.
/// These codes are independent of the ECU's eventual UDS positive/negative reply.
pub(super) enum AcknowledgementCode {
    Positive(u8),
    Negative(u8),
}

/// §7.4, Table 5: announcement fields before attaching the sender's IP address.
pub(super) struct Announcement {
    pub vin: [u8; 17],
    pub logical_address: u16,
    pub eid: [u8; 6],
    pub gid: [u8; 6],
    pub further_action: u8,
    pub vin_gid_sync_status: Option<u8>,
}

impl Message {
    /// §11.3, 4.DoIP-122: a UDP datagram contains exactly one DoIP message.
    pub fn from_datagram(bytes: &[u8], max_size: usize) -> Result<Self> {
        let header = Header::parse(bytes, max_size)?;
        if bytes.len() - HEADER_LEN != header.payload_len {
            return Err(invalid("UDP datagram length does not match DoIP header"));
        }
        Self::from_payload(header, &bytes[HEADER_LEN..])
    }

    fn from_payload(header: Header, bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader { remaining: bytes };
        let payload = match header.kind {
            HEADER_NACK => Payload::HeaderNack(reader.read_u8()?),
            ANNOUNCEMENT => Payload::Announcement(Announcement {
                vin: reader.read_array()?,
                logical_address: reader.read_u16()?,
                eid: reader.read_array()?,
                gid: reader.read_array()?,
                further_action: reader.read_u8()?,
                vin_gid_sync_status: reader.remaining.first().copied(),
            }),
            // §12.5, Table 55: the responding entity may be a gateway, whose
            // address differs from the target ECU's diagnostic address.
            ACTIVATION_RESPONSE => Payload::RoutingActivation {
                tester_address: reader.read_u16()?,
                entity_address: reader.read_u16()?,
                code: reader.read_u8()?,
            },
            ALIVE_REQUEST => Payload::AliveCheckRequest,
            DIAGNOSTIC => Payload::Diagnostic {
                addresses: LogicalAddresses::read(&mut reader)?,
                data: reader.remaining.to_vec(),
            },
            // §9.5, Tables 23 and 25: ACK/NACK addresses are reversed from the
            // request, followed by a code and an optional prefix of its data.
            ACK | NACK => {
                let addresses = LogicalAddresses::read(&mut reader)?;
                let code = reader.read_u8()?;
                Payload::DiagnosticAcknowledgement {
                    addresses,
                    code: if header.kind == ACK {
                        AcknowledgementCode::Positive(code)
                    } else {
                        AcknowledgementCode::Negative(code)
                    },
                    previous_data: reader.remaining.to_vec(),
                }
            }
            _ => Payload::Other,
        };
        Ok(Self {
            version: header.version,
            payload,
        })
    }
}

struct Header {
    version: u8,
    kind: u16,
    payload_len: usize,
}

impl Header {
    /// Payload lengths are checked against the configured limit and message type.
    fn parse(bytes: &[u8], max_size: usize) -> Result<Self> {
        if bytes.len() < HEADER_LEN {
            return Err(invalid("invalid protocol version or inverse version"));
        }
        let mut reader = Reader { remaining: bytes };
        let version = reader.read_u8()?;
        let inverse_version = reader.read_u8()?;
        if !(2..=4).contains(&version) || inverse_version != !version {
            return Err(invalid("invalid protocol version or inverse version"));
        }
        let kind = reader.read_u16()?;
        let payload_len = u32::from_be_bytes(reader.read_array()?) as usize;
        if payload_len > max_size {
            return Err(Error::PayloadTooLarge.into());
        }
        // Fixed layouts: §7.4 Table 5, §12.5 Table 55, §9.6 Table 27,
        // and §9.5 Tables 21, 23, 25. Unknown types are consumed and ignored.
        let valid = match kind {
            HEADER_NACK => payload_len == 1,
            ANNOUNCEMENT => matches!(payload_len, 32 | 33),
            ACTIVATION_RESPONSE => matches!(payload_len, 9 | 13),
            ALIVE_REQUEST => payload_len == 0,
            DIAGNOSTIC | ACK | NACK => payload_len >= 5,
            _ => true,
        };
        if !valid {
            return Err(invalid("invalid payload length for message type"));
        }
        Ok(Self {
            version,
            kind,
            payload_len,
        })
    }
}

/// §9.2: TCP can split one message across reads or coalesce several messages.
/// Incomplete bytes remain buffered until a later read completes the message.
pub(super) fn decode(bytes: &mut Vec<u8>, max_size: usize) -> Result<Option<Message>> {
    if bytes.len() < HEADER_LEN {
        return Ok(None);
    }
    let header = Header::parse(bytes, max_size)?;
    if bytes.len() - HEADER_LEN < header.payload_len {
        return Ok(None);
    }
    let end = HEADER_LEN + header.payload_len;
    let message = Message::from_payload(header, &bytes[HEADER_LEN..end])?;
    bytes.drain(..end);
    Ok(Some(message))
}

/// Read fixed-size fields in wire order using checked slice splits.
struct Reader<'a> {
    remaining: &'a [u8],
}

impl Reader<'_> {
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let (field, remaining) = self
            .remaining
            .split_first_chunk::<N>()
            .ok_or_else(|| invalid("truncated DoIP payload"))?;
        self.remaining = remaining;
        Ok(*field)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.read_array()?))
    }
}
