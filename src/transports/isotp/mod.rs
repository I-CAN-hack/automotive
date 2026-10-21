//! ISO Transport Protocol (ISO-TP) implementation, implements ISO 15765-2
//! ## Example:
//! ```rust
//! use automotive::StreamExt;
//! async fn isotp_example() -> automotive::Result<()> {
//!    let adapter = automotive::adapters::get_adapter()?;
//!    let config = automotive::transports::isotp::IsoTPConfig::builder(automotive::can::Identifier::standard(0x7a1)?)
//!        .build()?;
//!    let isotp = automotive::transports::isotp::IsoTPAdapter::new(&adapter, config);
//!
//!    let mut stream = isotp.recv(); // Create receiver before sending request
//!    isotp.send(&[0x3e, 0x00]).await?;
//!    let response = stream.next().await.ok_or(automotive::Error::Disconnected)??;
//!    Ok(())
//! }
//! ```

mod config;
mod constants;
mod error;
mod types;

pub use config::{IsoTPConfig, IsoTPConfigBuilder};
pub use constants::{FlowStatus, FrameType, FLOW_SATUS_MASK, FRAME_TYPE_MASK};
pub use error::Error;

use crate::can::AsyncCanAdapter;
use crate::can::{Frame, DLC_TO_LEN};
use crate::transports::TransportLayer;
use crate::Result;
use crate::{Stream, StreamExt, Timeout};
use arrayvec::ArrayVec;
use async_stream::stream;
use tracing::debug;

use self::types::FlowControlConfig;

const DEFAULT_TIMEOUT_MS: u64 = 100;

impl TransportLayer for IsoTPAdapter<'_> {
    fn send<'a>(
        &'a self,
        data: &'a [u8],
    ) -> impl std::future::Future<Output = crate::Result<()>> + 'a {
        IsoTPAdapter::send(self, data)
    }

    fn recv(&self) -> impl crate::Stream<Item = crate::Result<Vec<u8>>> + Unpin + '_ {
        IsoTPAdapter::recv(self)
    }
}

const DEFAULT_PADDING_BYTE: u8 = 0xCC;

/// N_WFTmax in ISO 15765-2
const MAX_WAIT_FC: usize = 10;

const CAN_MAX_DLEN: usize = 8;
const CAN_FD_MAX_DLEN: usize = 64;

const ISO_TP_MAX_DLEN: usize = (1 << 12) - 1;
const ISO_TP_FD_MAX_DLEN: usize = u32::MAX as usize;

/// Encode a separation-time value to the ISO 15765-2 STmin byte.
///
/// Values that do not land exactly on a representable STmin step round up to
/// the next valid encoding so the requested delay is never undershot.
///
/// Encoding (ISO 15765-2 §9.6.5.4 / Table 20):
/// - 0 µs -> `0x00` (no delay)
/// - 1-900 µs -> `0xF1`-`0xF9` (100 µs steps, rounded up)
/// - 901-127_000 µs -> `0x01`-`0x7F` (1 ms steps, rounded up)
/// - Values above 127 ms clamp to `0x7F`
pub fn duration_to_stmin_byte(duration: std::time::Duration) -> u8 {
    let us = duration
        .as_nanos()
        .div_ceil(1_000)
        .min(u128::from(u32::MAX)) as u32;
    if us == 0 {
        0x00
    } else if us < 1_000 {
        let steps = us.saturating_add(99) / 100;
        if steps <= 9 {
            0xF0 + steps as u8
        } else {
            0x01
        }
    } else {
        let ms = us.saturating_add(999) / 1_000;
        ms.min(127) as u8
    }
}

struct Reception {
    data: Vec<u8>,
    length: usize,
    rx_dl: usize,
    next_sn: u8,
    block_frames: u8,
}

/// Wraps a CAN adapter to send and receive ISO-TP messages over Classical CAN or CAN FD.
pub struct IsoTPAdapter<'a> {
    adapter: &'a AsyncCanAdapter,
    config: IsoTPConfig,
}

impl<'a> IsoTPAdapter<'a> {
    /// Convenience method for creating a new IsoTPAdapter from a CAN adapter and an Arbitration ID.
    ///
    /// # Panics
    /// Panics if the transmit ID exceeds 29 bits or the standard receive ID
    /// derived by [`IsoTPConfig::builder`] exceeds 11 bits.
    pub fn from_id(adapter: &'a AsyncCanAdapter, id: u32) -> Self {
        let config = IsoTPConfig::builder(id.try_into().expect("CAN identifier exceeds 29 bits"))
            .build()
            .expect("ISO-TP receive identifier exceeds 11 bits");
        Self::new(adapter, config)
    }

    /// Create a new IsoTPAdapter from a CAN adapter and a configuration.
    pub fn new(adapter: &'a AsyncCanAdapter, config: IsoTPConfig) -> Self {
        Self { adapter, config }
    }

    fn pad(&self, data: &mut ArrayVec<u8, CAN_FD_MAX_DLEN>) -> Result<()> {
        // Ensure we leave space for the extended address
        let offset = self.config.ext_address.is_some() as usize;
        let len = data.len() + offset;

        // Pad to at least 8 bytes if padding is enabled
        if let Some(padding) = self.config.padding {
            if len < CAN_MAX_DLEN {
                let padding_len = CAN_MAX_DLEN - len; // Offset for extended address is already accounted for
                data.extend(std::iter::repeat(padding).take(padding_len));
            }
        }

        // Pad to next valid DLC for CAN-FD
        if !DLC_TO_LEN.contains(&len) {
            let idx = DLC_TO_LEN
                .iter()
                .position(|&x| x > data.len())
                .ok_or(crate::Error::MalformedFrame)?;
            let padding = self.config.padding.unwrap_or(DEFAULT_PADDING_BYTE);
            let padding_len = DLC_TO_LEN[idx] - len;
            data.extend(std::iter::repeat(padding).take(padding_len));
        }
        Ok(())
    }

    /// Ofset from the start of the frame. 1 in case of extended address, 0 otherwise.
    fn offset(&self) -> usize {
        self.config.ext_address.is_some() as usize
    }

    /// Maximum data for a clasic CAN frame, taking into account space needed for the extended address.
    fn can_max_dlen(&self) -> usize {
        CAN_MAX_DLEN - self.offset()
    }

    /// Maximum data for a CAN-FD frame, taking into account space needed for the extended address.
    fn can_fd_max_dlen(&self) -> usize {
        CAN_FD_MAX_DLEN - self.offset()
    }

    /// Maximum data length for a CAN frame based on the current config
    fn max_can_data_length(&self) -> usize {
        match self.config.max_dlen {
            Some(dlen) => dlen - self.offset(),
            None => {
                if self.config.fd {
                    self.can_fd_max_dlen()
                } else {
                    self.can_max_dlen()
                }
            }
        }
    }

    /// Maximum data length for an ISO-TP packet based on the current config
    fn max_isotp_data_length(&self) -> usize {
        if self.config.fd {
            ISO_TP_FD_MAX_DLEN
        } else {
            ISO_TP_MAX_DLEN
        }
    }

    /// Build a CAN frame from the payload, inserting the extended address if needed.
    fn frame(&self, data: &mut ArrayVec<u8, CAN_FD_MAX_DLEN>) -> Result<Frame> {
        if let Some(ext_address) = self.config.ext_address {
            data.try_insert(0, ext_address)
                .map_err(|_| crate::Error::MalformedFrame)?;
        }

        Frame::builder(self.config.tx_id)
            .bus(self.config.bus)
            .data(data)
            .fd(self.config.fd)
            .build()
    }

    pub async fn send_single_frame(&self, data: &[u8]) -> Result<()> {
        let mut buf = ArrayVec::new();

        if data.len() < self.can_max_dlen() {
            // Len fits in classic CAN message
            buf.push(FrameType::Single as u8 | data.len() as u8);
        } else {
            // Use escape sequence for length, length is in the next byte
            buf.extend([FrameType::Single as u8, data.len() as u8]);
        }

        buf.try_extend_from_slice(data)
            .map_err(|_| crate::Error::MalformedFrame)?;
        self.pad(&mut buf)?;

        debug!("TX SF, length: {} data {}", data.len(), hex::encode(&buf));

        let frame = self.frame(&mut buf)?;
        self.adapter.send(&frame).await?;
        Ok(())
    }

    pub async fn send_first_frame(&self, data: &[u8]) -> Result<usize> {
        let mut buf = ArrayVec::new();
        if data.len() <= ISO_TP_MAX_DLEN {
            let b0: u8 = FrameType::First as u8 | ((data.len() >> 8) & 0xF) as u8;
            let b1: u8 = (data.len() & 0xFF) as u8;
            buf.extend([b0, b1]);
        } else {
            let b0: u8 = FrameType::First as u8;
            let b1: u8 = 0x00;
            buf.extend([b0, b1]);
            buf.extend((data.len() as u32).to_be_bytes());
        }
        let offset = buf.len();
        buf.try_extend_from_slice(&data[..self.max_can_data_length() - buf.len()])
            .map_err(|_| crate::Error::MalformedFrame)?;

        debug!("TX FF, length: {} data {}", data.len(), hex::encode(&buf));

        let frame = self.frame(&mut buf)?;
        self.adapter.send(&frame).await?;
        Ok(offset)
    }

    pub async fn send_consecutive_frame(&self, data: &[u8], idx: usize) -> Result<()> {
        let idx = ((idx + 1) & 0xF) as u8;

        let mut buf = ArrayVec::new();
        buf.push(FrameType::Consecutive as u8 | idx);
        buf.try_extend_from_slice(data)
            .map_err(|_| crate::Error::MalformedFrame)?;
        self.pad(&mut buf)?;

        debug!("TX CF, idx: {} data {}", idx, hex::encode(&buf));

        let frame = self.frame(&mut buf)?;

        self.adapter.send(&frame).await?;

        Ok(())
    }

    async fn receive_flow_control(
        &self,
        stream: &mut std::pin::Pin<&mut Timeout<impl Stream<Item = Frame>>>,
    ) -> Result<FlowControlConfig> {
        for _ in 0..MAX_WAIT_FC {
            let frame = stream.next().await.unwrap()?;

            // Remove extended address from frame
            let data = frame
                .data()
                .get(self.offset()..)
                .filter(|data| data.len() >= 3)
                .ok_or(crate::transports::isotp::error::Error::MalformedFrame)?;

            debug!("RX FC, data {}", hex::encode(data));

            // Check if Flow Control
            if FrameType::from_repr(data[0] & FRAME_TYPE_MASK) != Some(FrameType::FlowControl) {
                return Err(crate::transports::isotp::error::Error::FlowControl.into());
            };

            // Check Flow Status
            match FlowStatus::from_repr(data[0] & FLOW_SATUS_MASK) {
                Some(FlowStatus::ContinueToSend) => {} // Ok
                Some(FlowStatus::Wait) => continue,    // Wait for next flow control
                Some(FlowStatus::Overflow) => {
                    return Err(crate::transports::isotp::error::Error::Overflow.into())
                }
                None => return Err(crate::transports::isotp::error::Error::MalformedFrame.into()),
            };

            // Parse block size and separation time
            let config = types::FlowControlConfig::try_from(data)?;

            debug!("RX FC, {:?} data {}", config, hex::encode(data));
            return Ok(config);
        }

        Err(crate::transports::isotp::error::Error::TooManyFCWait.into())
    }

    async fn send_multiple(&self, data: &[u8]) -> Result<()> {
        // Stream for receiving flow control
        let stream = self
            .adapter
            .recv_filter(|frame| {
                if !frame.is_data()
                    || frame.bus != self.config.bus
                    || frame.id() != Some(self.config.rx_id)
                    || frame.loopback
                {
                    return false;
                }

                if let Some(address) = self.config.rx_ext_address.or(self.config.ext_address) {
                    return frame.data().first() == Some(&address);
                }

                true
            })
            .timeout(self.config.timeout);
        tokio::pin!(stream);

        let offset = self.send_first_frame(data).await?;
        let mut fc_config = self.receive_flow_control(&mut stream).await?;

        let tx_dl = self.max_can_data_length();
        let chunks = data[tx_dl - offset..].chunks(tx_dl - 1);
        let mut it = chunks.enumerate().peekable();
        let mut block_frames = 0u8;
        while let Some((idx, chunk)) = it.next() {
            self.send_consecutive_frame(chunk, idx).await?;

            if it.peek().is_none() {
                break;
            }
            // Each flow-control frame can specify a new block size and STmin.
            block_frames = block_frames.saturating_add(1);
            if fc_config.block_size != 0 && block_frames == fc_config.block_size {
                fc_config = self.receive_flow_control(&mut stream).await?;
                block_frames = 0;
            }
            let st_min = self
                .config
                .separation_time_min
                .unwrap_or(fc_config.separation_time_min);
            tokio::time::sleep(st_min).await;
        }

        Ok(())
    }

    /// Asynchronously send an ISO-TP frame of up to 4095 bytes. Returns Timeout if the ECU is not responding in time with flow control messages.
    pub async fn send(&self, data: &[u8]) -> Result<()> {
        debug!("TX {}", hex::encode(data));

        // Single frame has 1 byte of overhead for CAN, and 2 bytes for CAN-FD with escape sequence
        let fits_in_single_frame =
            data.len() < self.can_max_dlen() || data.len() < self.max_can_data_length() - 1;

        if fits_in_single_frame {
            self.send_single_frame(data).await?;
        } else if data.len() <= self.max_isotp_data_length() {
            self.send_multiple(data).await?;
        } else {
            return Err(crate::transports::isotp::error::Error::DataTooLarge.into());
        }

        Ok(())
    }

    fn recv_single_frame(&self, frame: &Frame) -> Option<Vec<u8>> {
        let address_length = self.offset();
        let can_dl = frame.data().len();
        let data = frame.data().get(address_length..)?;
        let short_length = usize::from(data.first()? & 0x0f);
        let (length, header_length) = if can_dl <= CAN_MAX_DLEN {
            // A zero nibble is reserved for Classical-size SingleFrames.
            if short_length == 0
                || (can_dl != CAN_MAX_DLEN && can_dl != short_length + 1 + address_length)
            {
                return None;
            }
            (short_length, 1)
        } else {
            // ISO 15765-2:2016, Tables 11 and 13: use the escape encoding
            // and only the padding needed for this CAN FD DLC.
            if short_length != 0 {
                return None;
            }
            let length = usize::from(*data.get(1)?);
            let dlc = DLC_TO_LEN.iter().position(|&length| length == can_dl)?;
            let minimum = (DLC_TO_LEN[dlc - 1] - 1 - address_length).max(8 - address_length);
            if length < minimum {
                return None;
            }
            (length, 2)
        };
        Some(data.get(header_length..header_length + length)?.to_vec())
    }

    fn recv_first_frame(&self, frame: &Frame) -> Option<Reception> {
        let rx_dl = frame.data().len();
        if rx_dl < CAN_MAX_DLEN {
            return None;
        }
        let data = frame.data().get(self.offset()..)?;
        let short_length = (usize::from(data.first()? & 0x0f) << 8) | usize::from(*data.get(1)?);
        let (length, header_length) = if short_length == 0 {
            let length = u32::from_be_bytes(data.get(2..6)?.try_into().ok()?) as usize;
            if length <= ISO_TP_MAX_DLEN {
                return None;
            }
            (length, 6)
        } else {
            (short_length, 2)
        };
        let minimum = if rx_dl == CAN_MAX_DLEN {
            8 - self.offset()
        } else {
            rx_dl - 1 - self.offset()
        };
        if length < minimum {
            return None;
        }
        Some(Reception {
            data: data.get(header_length..)?.to_vec(),
            length,
            rx_dl,
            next_sn: 1,
            block_frames: 0,
        })
    }

    async fn send_flow_control(&self) -> Result<()> {
        let mut flow_control = ArrayVec::new();
        flow_control.extend([
            0x30,
            self.config.block_size,
            duration_to_stmin_byte(self.config.receive_separation_time_min),
        ]);
        self.pad(&mut flow_control)?;
        debug!("TX FC, data {}", hex::encode(&flow_control));
        let frame = self.frame(&mut flow_control)?;
        self.adapter.send(&frame).await
    }

    fn recv_consecutive_frame(&self, frame: &Frame, reception: &mut Reception) -> Result<bool> {
        let can_dl = frame.data().len();
        let remaining = reception.length - reception.data.len();
        let capacity = reception.rx_dl - self.offset() - 1;
        if can_dl > reception.rx_dl {
            return Ok(false);
        }
        if remaining > capacity {
            if can_dl != reception.rx_dl {
                return Ok(false);
            }
        } else {
            let required = remaining + 1 + self.offset();
            let padded = *DLC_TO_LEN
                .iter()
                .find(|&&length| length >= required)
                .unwrap();
            if can_dl != padded && !(required <= CAN_MAX_DLEN && can_dl == CAN_MAX_DLEN) {
                return Ok(false);
            }
        }
        let data = &frame.data()[self.offset()..];
        if data[0] & 0x0f != reception.next_sn {
            return Err(Error::OutOfOrder.into());
        }
        let length = remaining.min(data.len() - 1);
        reception.data.extend_from_slice(&data[1..1 + length]);
        reception.next_sn = (reception.next_sn + 1) & 0x0f;
        Ok(true)
    }

    /// Receive one message, discarding invalid length encodings and CAN DLCs.
    async fn recv_from_stream(
        &self,
        stream: &mut std::pin::Pin<&mut Timeout<impl Stream<Item = Frame>>>,
    ) -> Result<Vec<u8>> {
        let mut reception: Option<Reception> = None;
        while let Some(frame) = stream.next().await {
            let frame = frame?;
            if frame.is_fd() != self.config.fd {
                continue;
            }
            let Some(data) = frame
                .data()
                .get(self.offset()..)
                .filter(|data| !data.is_empty())
            else {
                continue;
            };
            match FrameType::from_repr(data[0] & FRAME_TYPE_MASK) {
                Some(FrameType::Single) => {
                    if let Some(message) = self.recv_single_frame(&frame) {
                        return Ok(message);
                    }
                }
                Some(FrameType::First) => {
                    if let Some(next) = self.recv_first_frame(&frame) {
                        if reception.is_some() {
                            return Err(Error::OutOfOrder.into());
                        }
                        self.send_flow_control().await?;
                        reception = Some(next);
                    }
                }
                Some(FrameType::Consecutive) => {
                    if let Some(current) = &mut reception {
                        if !self.recv_consecutive_frame(&frame, current)? {
                            continue;
                        }
                        if current.data.len() == current.length {
                            return Ok(reception.take().unwrap().data);
                        }
                        current.block_frames = current.block_frames.saturating_add(1);
                        if self.config.block_size != 0
                            && current.block_frames == self.config.block_size
                        {
                            self.send_flow_control().await?;
                            current.block_frames = 0;
                        }
                    } else {
                        return Err(Error::OutOfOrder.into());
                    }
                }
                Some(FrameType::FlowControl) => {}
                _ => return Err(Error::UnknownFrameType.into()),
            }
        }
        Err(crate::Error::Disconnected)
    }

    /// Stream of ISO-TP packets. Can be used if multiple responses are expected from a single request. Returns Timeout if the timeout is exceeded between individual ISO-TP frames. Note the total time to receive a packet may be longer than the timeout.
    pub fn recv(&self) -> impl Stream<Item = Result<Vec<u8>>> + '_ {
        let stream = self
            .adapter
            .recv_filter(|frame| {
                if !frame.is_data()
                    || frame.bus != self.config.bus
                    || frame.id() != Some(self.config.rx_id)
                    || frame.loopback
                {
                    return false;
                }

                if let Some(address) = self.config.rx_ext_address.or(self.config.ext_address) {
                    return frame.data().first() == Some(&address);
                }

                true
            })
            .timeout(self.config.timeout);

        Box::pin(stream! {
            tokio::pin!(stream);

            loop {
                yield self.recv_from_stream(&mut stream).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::duration_to_stmin_byte;
    use std::time::Duration;

    #[test]
    fn duration_to_stmin_byte_rounds_up_to_supported_microsecond_steps() {
        assert_eq!(duration_to_stmin_byte(Duration::ZERO), 0x00);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(1)), 0xF1);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(99)), 0xF1);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(100)), 0xF1);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(101)), 0xF2);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(900)), 0xF9);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(901)), 0x01);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(999)), 0x01);
    }

    #[test]
    fn duration_to_stmin_byte_rounds_up_to_supported_millisecond_steps() {
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(1_000)), 0x01);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(1_001)), 0x02);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(1_999)), 0x02);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(2_000)), 0x02);
    }

    #[test]
    fn duration_to_stmin_byte_clamps_to_maximum_supported_delay() {
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(127_000)), 0x7F);
        assert_eq!(duration_to_stmin_byte(Duration::from_micros(127_001)), 0x7F);
        assert_eq!(duration_to_stmin_byte(Duration::from_secs(u64::MAX)), 0x7F);
    }
}
