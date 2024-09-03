//! ISO Transport Protocol (ISO-TP) implementation, implements ISO 15765-2
//! ## Example:
//! ```rust
//! use automotive::StreamExt;
//! async fn isotp_example() {
//!    let adapter = automotive::can::get_adapter().unwrap();
//!    let config = automotive::isotp::IsoTPConfig::default().tx(automotive::can::Identifier::Standard(0x7a1));
//!    let isotp = automotive::isotp::IsoTPAdapter::new(&adapter, config);
//!
//!    let mut stream = isotp.recv(); // Create receiver before sending request
//!    isotp.send(&[0x3e, 0x00]).await.unwrap();
//!    let response = stream.next().await.unwrap().unwrap();
//! }
//! ```

mod constants;
mod error;
mod types;

pub use constants::{FlowStatus, FrameType, FLOW_SATUS_MASK, FRAME_TYPE_MASK};
pub use error::Error;

use crate::can::AsyncCanAdapter;
use crate::can::{Frame, Identifier, DLC_TO_LEN};
use crate::Result;
use crate::{Stream, StreamExt, Timeout};
use async_stream::stream;
use tracing::debug;

use self::types::FlowControlConfig;

const DEFAULT_OFFSET: u32 = 0x8;
const DEFAULT_TIMEOUT_MS: u64 = 100;
const DEFAULT_PADDING_BYTE: u8 = 0xAA;

/// N_WFTmax in ISO 15765-2
const MAX_WAIT_FC: usize = 10;

const CAN_MAX_DLEN: usize = 8;
const CAN_FD_MAX_DLEN: usize = 64;

const ISO_TP_MAX_DLEN: usize = (1 << 12) - 1;
const ISO_TP_FD_MAX_DLEN: usize = (1 << 32) - 1;

/// Configuring passed to the IsoTPAdapter.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IsoTPConfig {
    bus_: u8,
    /// Transmit ID
    tx_: Identifier,
    /// Receive ID
    rx_: Identifier,
    /// Padding byte (0x00, or more efficient 0xAA). Set to None to disable padding.
    padding_: Option<u8>,
    /// Max timeout for receiving a frame
    timeout_: std::time::Duration,
    /// Override for Seperation Time (STmin) for transmitted frames
    separation_time_min_: Option<std::time::Duration>,
    /// Enable CAN-FD Mode
    fd_: bool,
    /// Extended address
    ext_address_: Option<u8>,
    /// Max data length. Will use default of 8 (CAN) or 64 (CAN-FD) if not set
    max_dlen_: Option<usize>,
}

impl Default for IsoTPConfig {
    fn default() -> IsoTPConfig {
        IsoTPConfig {
            bus_: 0,
            tx_: Identifier::Standard(0),
            rx_: Identifier::Standard(0),
            padding_: Some(DEFAULT_PADDING_BYTE),
            timeout_: std::time::Duration::from_millis(DEFAULT_TIMEOUT_MS),
            separation_time_min_: None,
            fd_: false,
            ext_address_: None,
            max_dlen_: None,
        }
    }
}

impl IsoTPConfig {
    pub fn bus(mut self, bus: u8) -> Self {
        self.bus_ = bus;
        self
    }

    pub fn tx(mut self, id: Identifier) -> Self {
        self.tx_ = id;

        // Set rx_id using default offset logic
        self.rx_ = match id {
            Identifier::Standard(id) => Identifier::Standard(id + DEFAULT_OFFSET),
            Identifier::Extended(id) => {
                let bytes = id.to_be_bytes();
                let id = u32::from_be_bytes([bytes[0], bytes[1], bytes[3], bytes[2]]); // Swap last two bytes
                Identifier::Extended(id)
            }
        };

        self
    }

    pub fn rx_offset(mut self, offset: u32) -> Self {
        self.rx_ = match self.tx_ {
            Identifier::Standard(id) => Identifier::Standard(id + offset),
            Identifier::Extended(_) => panic!("Extended IDs do not support offset"),
        };
        self
    }

    pub fn rx(mut self, id: Identifier) -> Self {
        self.rx_ = id;
        self
    }

    pub fn padding(mut self, padding: Option<u8>) -> Self {
        self.padding_ = padding;
        self
    }

    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout_ = timeout;
        self
    }

    pub fn separation_time_min(mut self, separation_time_min: Option<std::time::Duration>) -> Self {
        self.separation_time_min_ = separation_time_min;
        self
    }

    pub fn fd(mut self, fd: bool) -> Self {
        self.fd_ = fd;
        self
    }

    pub fn ext_address(mut self, ext_address: Option<u8>) -> Self {
        self.ext_address_ = ext_address;
        self
    }

    pub fn max_dlen(mut self, max_dlen: Option<usize>) -> Self {
        self.max_dlen_ = max_dlen;
        self
    }
}

/// Wraps a CAN adapter to provide a simple interface for sending and receiving ISO-TP frames. CAN-FD ISO-TP is currently not supported.
pub struct IsoTPAdapter<'a> {
    adapter: &'a AsyncCanAdapter,
    config: IsoTPConfig,
}

impl<'a> IsoTPAdapter<'a> {
    /// Create a new IsoTPAdapter from a CAN adapter and a configuration.
    pub fn new(adapter: &'a AsyncCanAdapter, config: IsoTPConfig) -> Self {
        Self { adapter, config }
    }

    fn pad(&self, data: &mut Vec<u8>) {
        // Ensure we leave space for the extended address
        let offset = self.config.ext_address_.is_some() as usize;
        let len = data.len() + offset;

        // Pad to at least 8 bytes if padding is enabled
        if let Some(padding) = self.config.padding_ {
            if len < CAN_MAX_DLEN {
                let padding_len = CAN_MAX_DLEN - len; // Offset for extended address is already accounted for
                data.extend(std::iter::repeat(padding).take(padding_len));
            }
        }

        // Pad to next valid DLC for CAN-FD
        if !DLC_TO_LEN.contains(&len) {
            let idx = DLC_TO_LEN.iter().position(|&x| x > data.len()).unwrap();
            let padding = self.config.padding_.unwrap_or(DEFAULT_PADDING_BYTE);
            let padding_len = DLC_TO_LEN[idx] - len;
            data.extend(std::iter::repeat(padding).take(padding_len));
        }
    }

    /// Ofset from the start of the frame. 1 in case of extended address, 0 otherwise.
    fn offset(&self) -> usize {
        self.config.ext_address_.is_some() as usize
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
        match self.config.max_dlen_ {
            Some(dlen) => dlen - self.offset(),
            None => {
                if self.config.fd_ {
                    self.can_fd_max_dlen()
                } else {
                    self.can_max_dlen()
                }
            }
        }
    }

    /// Maximum data length for an ISO-TP packet based on the current config
    fn max_isotp_data_length(&self) -> usize {
        if self.config.fd_ {
            ISO_TP_FD_MAX_DLEN
        } else {
            ISO_TP_MAX_DLEN
        }
    }

    /// Build a CAN frame from the payload. Inserts extended address and padding if needed.
    fn frame(&self, data: &[u8]) -> Result<Frame> {
        let mut data = data.to_vec();

        if let Some(ext_address) = self.config.ext_address_ {
            data.insert(0, ext_address);
        }

        // Check if the data length is valid
        if !DLC_TO_LEN.contains(&data.len()) {
            println!("len {}", data.len());
            return Err(crate::Error::MalformedFrame);
        }

        let frame = Frame {
            bus: self.config.bus_,
            id: self.config.tx_,
            data,
            loopback: false,
            fd: self.config.fd_,
        };

        Ok(frame)
    }

    pub async fn send_single_frame(&self, data: &[u8]) -> Result<()> {
        let mut buf;

        if data.len() < self.can_max_dlen() {
            // Len fits in classic CAN message
            buf = vec![FrameType::Single as u8 | data.len() as u8];
        } else {
            // Use escape sequence for length, length is in the next byte
            buf = vec![FrameType::Single as u8, data.len() as u8];
        }

        buf.extend(data);
        self.pad(&mut buf);

        debug!("TX SF, length: {} data {}", data.len(), hex::encode(&buf));

        let frame = self.frame(&buf)?;
        self.adapter.send(&frame).await;
        Ok(())
    }

    pub async fn send_first_frame(&self, data: &[u8]) -> Result<usize> {
        let mut buf;
        if data.len() <= ISO_TP_MAX_DLEN {
            let b0: u8 = FrameType::First as u8 | ((data.len() >> 8) & 0xF) as u8;
            let b1: u8 = (data.len() & 0xFF) as u8;
            buf = vec![b0, b1];
        } else {
            let b0: u8 = FrameType::First as u8;
            let b1: u8 = 0x00;
            buf = vec![b0, b1];
            buf.extend((data.len() as u32).to_be_bytes());
        }
        let offset = buf.len();
        buf.extend(&data[..self.max_can_data_length() - buf.len()]);

        debug!("TX FF, length: {} data {}", data.len(), hex::encode(&buf));

        let frame = self.frame(&buf)?;
        self.adapter.send(&frame).await;
        Ok(offset)
    }

    pub async fn send_consecutive_frame(&self, data: &[u8], idx: usize) -> Result<()> {
        let idx = ((idx + 1) & 0xF) as u8;

        let mut buf = vec![FrameType::Consecutive as u8 | idx];
        buf.extend(data);
        self.pad(&mut buf);

        debug!("TX CF, idx: {} data {}", idx, hex::encode(&buf));

        let frame = self.frame(&buf)?;

        self.adapter.send(&frame).await;

        Ok(())
    }

    async fn receive_flow_control(
        &self,
        stream: &mut std::pin::Pin<&mut Timeout<impl Stream<Item = Frame>>>,
    ) -> Result<FlowControlConfig> {
        for _ in 0..MAX_WAIT_FC {
            let mut frame = stream.next().await.unwrap()?;

            // Remove extended address from frame
            frame.data = frame.data.split_off(self.offset());

            debug!("RX FC, data {}", hex::encode(&frame.data));

            // Check if Flow Control
            if FrameType::from_repr(frame.data[0] & FRAME_TYPE_MASK) != Some(FrameType::FlowControl)
            {
                return Err(crate::isotp::error::Error::FlowControl.into());
            };

            // Check Flow Status
            match FlowStatus::from_repr(frame.data[0] & FLOW_SATUS_MASK) {
                Some(FlowStatus::ContinueToSend) => {} // Ok
                Some(FlowStatus::Wait) => continue,    // Wait for next flow control
                Some(FlowStatus::Overflow) => {
                    return Err(crate::isotp::error::Error::Overflow.into())
                }
                None => return Err(crate::isotp::error::Error::MalformedFrame.into()),
            };

            // Parse block size and separation time
            let config = types::FlowControlConfig::try_from(&frame)?;

            debug!("RX FC, {:?} data {}", config, hex::encode(&frame.data));
            return Ok(config);
        }

        Err(crate::isotp::error::Error::TooManyFCWait.into())
    }

    async fn send_multiple(&self, data: &[u8]) -> Result<()> {
        // Stream for receiving flow control
        let stream = self
            .adapter
            .recv_filter(|frame| {
                if frame.id != self.config.rx_ || frame.loopback {
                    return false;
                }

                if self.config.ext_address_.is_some() {
                    return frame.data.first() == self.config.ext_address_.as_ref();
                }

                true
            })
            .timeout(self.config.timeout_);
        tokio::pin!(stream);

        let offset = self.send_first_frame(data).await?;
        let mut fc_config = self.receive_flow_control(&mut stream).await?;

        // Check for separation time override
        let st_min = match self.config.separation_time_min_ {
            Some(st_min) => st_min,
            None => fc_config.separation_time_min,
        };

        let tx_dl = self.max_can_data_length();
        let chunks = data[tx_dl - offset..].chunks(tx_dl - 1);
        let mut it = chunks.enumerate().peekable();
        while let Some((idx, chunk)) = it.next() {
            self.send_consecutive_frame(chunk, idx).await?;

            // Wait for flow control every `block_size` frames, except for the first frame
            if fc_config.block_size != 0 && idx > 0 && idx % fc_config.block_size as usize == 0 {
                // Wait for next flow control
                fc_config = self.receive_flow_control(&mut stream).await?;
            } else {
                // Sleep for separation time between frames
                let last = it.peek().is_none();
                if !last {
                    tokio::time::sleep(st_min).await;
                }
            }
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
            return Err(crate::isotp::error::Error::DataTooLarge.into());
        }

        Ok(())
    }

    async fn recv_single_frame(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut len = (data[0] & 0xF) as usize;
        let mut offset = 1;

        // CAN-FD Escape sequence
        if len == 0 {
            len = data[1] as usize;
            offset = 2;
        }

        // Check if the frame contains enough data
        if len + offset > data.len() {
            return Err(crate::isotp::error::Error::MalformedFrame.into());
        }

        debug!("RX SF, length: {} data {}", len, hex::encode(data));

        Ok(data[offset..len + offset].to_vec())
    }

    async fn recv_first_frame(&self, data: &[u8], buf: &mut Vec<u8>) -> Result<usize> {
        let b0 = data[0] as u16;
        let b1 = data[1] as u16;
        let mut len = ((b0 << 8 | b1) & 0xFFF) as usize;
        let mut offset = 2;

        // CAN-FD Escape sequence
        if len == 0 {
            offset = 6;
            len = u32::from_be_bytes([data[2], data[3], data[4], data[5]]) as usize;
        }
        debug!("RX FF, length: {}, data {}", len, hex::encode(data));

        // A FF cannot use CAN frame data optmization, and always needs to be full length.
        if data.len() < self.max_can_data_length() {
            return Err(crate::isotp::error::Error::MalformedFrame.into());
        }

        buf.extend(&data[offset..]);

        // Send Flow Control
        let mut flow_control = vec![0x30, 0x00, 0x00];
        self.pad(&mut flow_control);

        debug!("TX FC, data {}", hex::encode(&flow_control));

        let frame = self.frame(&flow_control)?;
        self.adapter.send(&frame).await;

        Ok(len)
    }

    async fn recv_consecutive_frame(
        &self,
        data: &[u8],
        buf: &mut Vec<u8>,
        len: usize,
        idx: u8,
    ) -> Result<u8> {
        let msg_idx = data[0] & 0xF;
        let remaining_len = len - buf.len();

        // Only the last consecutive frame can use CAN frame data optimization
        let tx_dl = self.max_can_data_length();
        if remaining_len >= tx_dl - 1 {
            // Ensure frame is full length
            if data.len() < tx_dl {
                return Err(crate::isotp::error::Error::MalformedFrame.into());
            }
        } else {
            // Ensure frame is long enough to contain the remaining data
            if data.len() - 1 < remaining_len {
                return Err(crate::isotp::error::Error::MalformedFrame.into());
            }
        }

        let end_idx = std::cmp::min(remaining_len + 1, data.len());

        buf.extend(&data[1..end_idx]);
        debug!(
            "RX CF, idx: {}, data {} {}",
            idx,
            hex::encode(data),
            hex::encode(&buf)
        );

        if msg_idx != idx {
            return Err(crate::isotp::error::Error::OutOfOrder.into());
        }

        let new_idx = if idx == 0xF { 0 } else { idx + 1 };
        Ok(new_idx)
    }

    /// Helper function to receive a single ISO-TP packet from the provided CAN stream.
    async fn recv_from_stream(
        &self,
        stream: &mut std::pin::Pin<&mut Timeout<impl Stream<Item = Frame>>>,
    ) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let mut len: Option<usize> = None;
        let mut idx: u8 = 1;

        while let Some(frame) = stream.next().await {
            // Remove extended address from frame
            let data = &frame?.data[self.offset()..];

            match FrameType::from_repr(data[0] & FRAME_TYPE_MASK) {
                Some(FrameType::Single) => {
                    return self.recv_single_frame(data).await;
                }
                Some(FrameType::First) => {
                    // If we already received a first frame, something went wrong
                    if len.is_some() {
                        return Err(Error::OutOfOrder.into());
                    }
                    len = Some(self.recv_first_frame(data, &mut buf).await?);
                }
                Some(FrameType::Consecutive) => {
                    if let Some(len) = len {
                        idx = self
                            .recv_consecutive_frame(data, &mut buf, len, idx)
                            .await?;
                        if buf.len() >= len {
                            return Ok(buf);
                        }
                    } else {
                        return Err(Error::OutOfOrder.into());
                    }
                }
                Some(FrameType::FlowControl) => {} // Ignore flow control frames, these are from a simultaneous transmission
                _ => {
                    return Err(Error::UnknownFrameType.into());
                }
            };
        }
        unreachable!();
    }

    /// Stream of ISO-TP packets. Can be used if multiple responses are expected from a single request. Returns Timeout if the timeout is exceeded between individual ISO-TP frames. Note the total time to receive a packet may be longer than the timeout.
    pub fn recv(&self) -> impl Stream<Item = Result<Vec<u8>>> + '_ {
        let stream = self
            .adapter
            .recv_filter(|frame| {
                if frame.id != self.config.rx_ || frame.loopback {
                    return false;
                }

                if self.config.ext_address_.is_some() {
                    return frame.data.first() == self.config.ext_address_.as_ref();
                }

                true
            })
            .timeout(self.config.timeout_);

        Box::pin(stream! {
            tokio::pin!(stream);

            loop {
                yield self.recv_from_stream(&mut stream).await;
            }
        })
    }
}
