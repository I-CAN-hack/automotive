//! ISO Transport Protocol (ISO-TP) implementation, implements ISO 15765-2
//! ## Example:
//! ```rust
//! use automotive::StreamExt;
//! async fn isotp_example() -> automotive::Result<()> {
//!    let adapter = automotive::adapters::get_adapter()?;
//!    let config = automotive::transports::isotp::IsoTPConfig::builder(automotive::can::Identifier::standard(0x7a1)?)
//!        .build()?;
//!    let isotp = automotive::transports::isotp::IsoTP::new(&adapter, config);
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
use crate::transports::Transport;
use crate::Result;
use crate::{Stream, StreamExt};
use arrayvec::ArrayVec;
use async_stream::stream;
use tracing::debug;

use self::types::FlowControlConfig;

const DEFAULT_TIMEOUT_MS: u64 = 1000;

impl Transport for IsoTP<'_> {
    fn send<'a>(
        &'a self,
        data: &'a [u8],
    ) -> impl std::future::Future<Output = crate::Result<()>> + 'a {
        IsoTP::send(self, data)
    }

    fn recv(&self) -> impl crate::Stream<Item = crate::Result<Vec<u8>>> + Unpin + '_ {
        IsoTP::recv(self)
    }
}

const DEFAULT_PADDING_BYTE: u8 = 0xCC;

const CAN_MAX_DLEN: usize = 8;
const CAN_FD_MAX_DLEN: usize = 64;

const ISO_TP_MAX_DLEN: usize = (1 << 12) - 1;
const ISO_TP_FD_MAX_DLEN: usize = u32::MAX as usize;
// Bound queued frames even when the peer permits an unlimited flow-control block.
const MAX_TX_BATCH: usize = 128;

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
#[derive(Clone)]
pub struct IsoTP<'a> {
    adapter: crate::shared::Shared<'a, AsyncCanAdapter>,
    config: IsoTPConfig,
}

impl<'a> IsoTP<'a> {
    /// Create an ISO-TP transport from a CAN adapter and an arbitration ID.
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

    /// Create an ISO-TP transport from a CAN adapter and a configuration.
    pub fn new(adapter: &'a AsyncCanAdapter, config: IsoTPConfig) -> Self {
        Self {
            adapter: crate::shared::Shared::Borrowed(adapter),
            config,
        }
    }

    /// Create a transport retaining shared ownership of its CAN adapter.
    pub fn new_owned(
        adapter: std::sync::Arc<AsyncCanAdapter>,
        config: IsoTPConfig,
    ) -> IsoTP<'static> {
        IsoTP {
            adapter: crate::shared::Shared::Owned(adapter),
            config,
        }
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

    fn max_single_frame_length(&self) -> usize {
        let header_length = if self.max_can_data_length() + self.offset() > CAN_MAX_DLEN {
            2
        } else {
            1
        };
        self.max_can_data_length() - header_length
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
            .brs(self.config.fd && self.config.brs.unwrap_or(self.adapter.default_brs()))
            .build()
    }

    fn accepts(&self, frame: &Frame) -> bool {
        frame.is_data()
            && !frame.loopback
            && frame.bus == self.config.bus
            && frame.id() == Some(self.config.rx_id)
            && frame.is_fd() == self.config.fd
            && self
                .config
                .rx_ext_address
                .or(self.config.ext_address)
                .is_none_or(|address| frame.data().first() == Some(&address))
    }

    async fn transmit_frame(&self, frame: &Frame) -> Result<()> {
        // N_As for data frames and N_Ar for flow control bound the wait for
        // CAN transmit confirmation, independently of subsequent reception.
        tokio::time::timeout(self.config.timeout, self.adapter.send(frame))
            .await
            .map_err(|_| crate::Error::Timeout)?
    }

    pub async fn send_single_frame(&self, data: &[u8]) -> Result<()> {
        self.config.validate()?;
        if data.is_empty() {
            return Err(Error::EmptyPayload.into());
        }
        if data.len() > self.max_single_frame_length() {
            return Err(Error::DataTooLarge.into());
        }
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
        self.transmit_frame(&frame).await?;
        Ok(())
    }

    pub async fn send_first_frame(&self, data: &[u8]) -> Result<usize> {
        self.config.validate()?;
        if data.len() <= self.max_single_frame_length() {
            return Err(Error::MalformedFrame.into());
        }
        if data.len() > self.max_isotp_data_length() {
            return Err(Error::DataTooLarge.into());
        }
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
        self.transmit_frame(&frame).await?;
        Ok(offset)
    }

    pub async fn send_consecutive_frame(&self, data: &[u8], idx: usize) -> Result<()> {
        let frame = self.consecutive_frame(data, idx)?;
        self.transmit_frame(&frame).await
    }

    fn consecutive_frame(&self, data: &[u8], idx: usize) -> Result<Frame> {
        self.config.validate()?;
        if data.is_empty() {
            return Err(Error::EmptyPayload.into());
        }
        if data.len() >= self.max_can_data_length() {
            return Err(Error::DataTooLarge.into());
        }
        let idx = (idx.wrapping_add(1) & 0xF) as u8;

        let mut buf = ArrayVec::new();
        buf.push(FrameType::Consecutive as u8 | idx);
        buf.try_extend_from_slice(data)
            .map_err(|_| crate::Error::MalformedFrame)?;
        self.pad(&mut buf)?;

        debug!("TX CF, idx: {} data {}", idx, hex::encode(&buf));

        self.frame(&mut buf)
    }

    /// Admit a zero-STmin batch in order, then await every CAN confirmation.
    /// Waiting after each individual frame would serialize USB echo latency.
    async fn transmit_frames(&self, frames: &[Frame]) -> Result<()> {
        use std::future::{poll_fn, Future};
        use std::task::Poll;

        let mut sends: Vec<_> = frames
            .iter()
            .map(|frame| Some(Box::pin(self.transmit_frame(frame))))
            .collect();
        poll_fn(|cx| {
            let mut pending = false;
            for send in &mut sends {
                let Some(future) = send else { continue };
                match future.as_mut().poll(cx) {
                    Poll::Ready(result) => {
                        *send = None;
                        if let Err(error) = result {
                            return Poll::Ready(Err(error));
                        }
                    }
                    Poll::Pending => pending = true,
                }
            }
            if pending {
                Poll::Pending
            } else {
                Poll::Ready(Ok(()))
            }
        })
        .await
    }

    fn transmit_separation_time(
        &self,
        fc_config: &FlowControlConfig,
        fallback_stmin: bool,
    ) -> std::time::Duration {
        self.config
            .separation_time_min
            .unwrap_or(if fallback_stmin {
                std::time::Duration::from_millis(127)
            } else {
                fc_config.separation_time_min
            })
    }

    async fn receive_flow_control(
        &self,
        stream: &mut (impl Stream<Item = Frame> + Unpin),
    ) -> Result<FlowControlConfig> {
        // N_Bs begins after the preceding FF/CF was confirmed. Only WAIT
        // restarts it; unrelated or truncated PDUs do not extend the deadline.
        let mut deadline = tokio::time::Instant::now() + self.config.timeout;
        loop {
            let frame = tokio::time::timeout_at(deadline, stream.next())
                .await
                .map_err(|_| crate::Error::Timeout)?
                .ok_or(crate::Error::Disconnected)?;
            let Some(data) = frame
                .data()
                .get(self.offset()..)
                .filter(|data| data.len() >= 3)
            else {
                continue;
            };
            if data[0] & FRAME_TYPE_MASK != FrameType::FlowControl as u8 {
                continue;
            }
            match FlowStatus::from_repr(data[0] & FLOW_SATUS_MASK) {
                Some(FlowStatus::ContinueToSend) => return FlowControlConfig::try_from(data),
                Some(FlowStatus::Wait) => {
                    // N_WFTmax limits a receiver's WAIT transmissions, not
                    // how many WAITs a sender may receive.
                    deadline = tokio::time::Instant::now() + self.config.timeout;
                }
                Some(FlowStatus::Overflow) => return Err(Error::Overflow.into()),
                None => return Err(Error::MalformedFrame.into()),
            }
        }
    }

    async fn send_multiple(&self, data: &[u8]) -> Result<()> {
        // Stream for receiving flow control
        let stream = self.adapter.recv_filter(|frame| self.accepts(frame));
        tokio::pin!(stream);

        let offset = self.send_first_frame(data).await?;
        let mut fc_config = self.receive_flow_control(&mut stream).await?;
        let mut fallback_stmin = fc_config.reserved_stmin;

        let tx_dl = self.max_can_data_length();
        let chunks = data[tx_dl - offset..].chunks(tx_dl - 1);
        let mut it = chunks.enumerate().peekable();
        let mut block_frames = 0u8;
        while let Some((idx, chunk)) = it.next() {
            if self.config.batch_send
                && self
                    .transmit_separation_time(&fc_config, fallback_stmin)
                    .is_zero()
            {
                let mut frames = vec![self.consecutive_frame(chunk, idx)?];
                block_frames = block_frames.saturating_add(1);
                while frames.len() < MAX_TX_BATCH
                    && (fc_config.block_size == 0 || block_frames < fc_config.block_size)
                {
                    let Some((idx, chunk)) = it.next() else { break };
                    frames.push(self.consecutive_frame(chunk, idx)?);
                    block_frames = block_frames.saturating_add(1);
                }
                self.transmit_frames(&frames).await?;
            } else {
                self.send_consecutive_frame(chunk, idx).await?;
                block_frames = block_frames.saturating_add(1);
            }

            if it.peek().is_none() {
                break;
            }
            // Each flow-control frame can specify a new block size and STmin.
            if fc_config.block_size != 0 && block_frames == fc_config.block_size {
                fc_config = self.receive_flow_control(&mut stream).await?;
                fallback_stmin |= fc_config.reserved_stmin;
                block_frames = 0;
            }
            let st_min = self.transmit_separation_time(&fc_config, fallback_stmin);
            if !st_min.is_zero() {
                tokio::time::sleep(st_min).await;
            }
        }

        Ok(())
    }

    /// Send a non-empty ISO-TP message, segmented according to TX_DL.
    /// Returns Timeout if a CAN transmit confirmation or required flow-control
    /// frame does not arrive within the configured timeout.
    pub async fn send(&self, data: &[u8]) -> Result<()> {
        debug!("TX {}", hex::encode(data));

        self.config.validate()?;
        if data.is_empty() {
            return Err(Error::EmptyPayload.into());
        }
        if data.len() <= self.max_single_frame_length() {
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
        self.transmit_frame(&frame).await
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

    /// Stream of ISO-TP messages, including multiple responses to one request.
    /// The timeout applies while idle, while confirming outgoing flow control,
    /// and while awaiting the next valid consecutive frame. Ignored traffic does
    /// not restart a deadline. A replacement SF/FF yields UnexpectedPdu before
    /// processing the replacement message.
    /// Idle expiry yields [`crate::Error::IdleTimeout`]; a timeout during
    /// transmission or reassembly yields [`crate::Error::Timeout`].
    pub fn recv(&self) -> impl Stream<Item = Result<Vec<u8>>> + '_ {
        self.clone().into_recv()
    }

    /// Subscribe immediately and return a stream owning this transport.
    ///
    /// A transport constructed with `new_owned` produces a `'static` stream.
    pub fn into_recv(self) -> impl Stream<Item = Result<Vec<u8>>> + Unpin + 'a {
        let filter = self.clone();
        let stream = self.adapter.recv_filter(move |frame| filter.accepts(frame));

        Box::pin(stream! {
            if let Err(error) = self.config.validate() {
                yield Err(error);
                return;
            }
            tokio::pin!(stream);
            let mut reception: Option<Reception> = None;
            let mut deadline = tokio::time::Instant::now() + self.config.timeout;
            loop {
                let frame = match tokio::time::timeout_at(deadline, stream.next()).await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => break,
                    Err(_) => {
                        let error = if reception.take().is_some() {
                            crate::Error::Timeout
                        } else {
                            crate::Error::IdleTimeout
                        };
                        deadline = tokio::time::Instant::now() + self.config.timeout;
                        yield Err(error);
                        continue;
                    }
                };
                let Some(data) = frame.data().get(self.offset()..).filter(|data| !data.is_empty()) else {
                    continue;
                };
                match FrameType::from_repr(data[0] & FRAME_TYPE_MASK) {
                    Some(FrameType::Single) => {
                        if let Some(message) = self.recv_single_frame(&frame) {
                            deadline = tokio::time::Instant::now() + self.config.timeout;
                            if reception.take().is_some() {
                                yield Err(Error::UnexpectedPdu.into());
                            }
                            yield Ok(message);
                        }
                    }
                    Some(FrameType::First) => {
                        if let Some(next) = self.recv_first_frame(&frame) {
                            let interrupted = reception.replace(next).is_some();
                            let result = self.send_flow_control().await;
                            // Start N_Cr after the FC transmit confirmation.
                            deadline = tokio::time::Instant::now() + self.config.timeout;
                            if interrupted {
                                yield Err(Error::UnexpectedPdu.into());
                            }
                            if let Err(error) = result {
                                reception = None;
                                yield Err(error);
                            }
                        }
                    }
                    Some(FrameType::Consecutive) => {
                        let Some(current) = &mut reception else { continue };
                        match self.recv_consecutive_frame(&frame, current) {
                            Err(error) => {
                                reception = None;
                                deadline = tokio::time::Instant::now() + self.config.timeout;
                                yield Err(error);
                            }
                            Ok(false) => {}
                            Ok(true) => {
                                deadline = tokio::time::Instant::now() + self.config.timeout;
                                if current.data.len() == current.length {
                                    yield Ok(reception.take().unwrap().data);
                                } else {
                                    current.block_frames = current.block_frames.saturating_add(1);
                                    if self.config.block_size != 0 && current.block_frames == self.config.block_size {
                                        current.block_frames = 0;
                                        let result = self.send_flow_control().await;
                                        deadline = tokio::time::Instant::now() + self.config.timeout;
                                        if let Err(error) = result {
                                            reception = None;
                                            yield Err(error);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Table 23: idle CFs, unexpected FCs and unknown PDUs are ignored.
                    _ => {}
                }
            }
            yield Err(crate::Error::Disconnected);
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
