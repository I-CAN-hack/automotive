//! ISO Transport Protocol (ISO-TP) implementation, implements ISO 15765-2
//! ## Example:
//! ```rust
//! use futures_util::stream::StreamExt;
//! async fn isotp_example() {
//!    let adapter = automotive::adapter::get_adapter().unwrap();
//!    let config = automotive::isotp::IsoTPConfig::new(0, automotive::can::Identifier::Standard(0x7a1));
//!    let isotp = automotive::isotp::IsoTPAdapter::new(&adapter, config);
//!
//!    let mut stream = isotp.recv(); // Create receiver before sending request
//!    isotp.send(&[0x3e, 0x00]).await.unwrap();
//!    let response = stream.next().await.unwrap().unwrap();
//! }
//! ```

pub mod constants;
pub mod error;
pub mod types;

use crate::async_can::AsyncCanAdapter;
use crate::can::Frame;
use crate::can::Identifier;
use crate::error::Error;
use crate::isotp::constants::FlowStatus;
use crate::isotp::constants::FLOW_SATUS_MASK;
use crate::isotp::constants::{FrameType, FRAME_TYPE_MASK};

use async_stream::stream;
use futures_core::stream::Stream;
use tokio_stream::{StreamExt, Timeout};
use tracing::debug;

use self::types::FlowControlConfig;

const DEFAULT_TIMEOUT_MS: u64 = 100;

/// Configuring passed to the IsoTPAdapter.
#[derive(Debug, Clone, Copy)]
pub struct IsoTPConfig {
    pub bus: u8,
    /// Transmit ID
    pub tx_id: Identifier,
    /// Receive ID
    pub rx_id: Identifier,
    /// Transmit Data Length
    pub tx_dl: usize,
    /// Padding byte (0x00, or more efficient 0xAA). Set to None to disable padding.
    pub padding: Option<u8>,
    /// Max timeout for receiving a frame
    pub timeout: std::time::Duration,
    /// Override for Seperation Time (STmin) for transmitted frames
    pub separation_time_min: Option<std::time::Duration>,
}

impl IsoTPConfig {
    pub fn new(bus: u8, id: Identifier) -> Self {
        let tx_id = id;
        let rx_id = match id {
            Identifier::Standard(id) => Identifier::Standard(id + 8),
            Identifier::Extended(id) => {
                let bytes = id.to_be_bytes();
                let id = u32::from_be_bytes([bytes[0], bytes[1], bytes[3], bytes[2]]); // Swap last two bytes
                Identifier::Extended(id)
            }
        };

        Self::new_from_tx_rx(bus, tx_id, rx_id)
    }

    pub fn new_from_offset(bus: u8, id: Identifier, offset: u32) -> Self {
        let tx_id = id;
        let rx_id = match id {
            Identifier::Standard(id) => Identifier::Standard(id + offset),
            Identifier::Extended(_) => panic!("Extended IDs do not support offset"),
        };

        Self::new_from_tx_rx(bus, tx_id, rx_id)
    }

    pub fn new_from_tx_rx(bus: u8, tx_id: Identifier, rx_id: Identifier) -> Self {
        Self {
            bus,
            tx_id,
            rx_id,
            tx_dl: 8,
            padding: Some(0xaa),
            timeout: std::time::Duration::from_millis(DEFAULT_TIMEOUT_MS),
            separation_time_min: None,
        }
    }
}

/// Wraps a CAN adapter to provide a simple interface for sending and receiving ISO-TP frames. CAN-FD ISO-TP is currently not supported.
pub struct IsoTPAdapter<'a> {
    adapter: &'a AsyncCanAdapter,
    config: IsoTPConfig,
}

impl<'a> IsoTPAdapter<'a> {
    /// Convenience method for creating a new IsoTPAdapter from a CAN adapter and an Arbitration ID.
    pub fn from_id(adapter: &'a AsyncCanAdapter, id: u32) -> Self {
        let config = IsoTPConfig::new(0, id.into());
        Self::new(adapter, config)
    }

    /// Create a new IsoTPAdapter from a CAN adapter and a configuration.
    pub fn new(adapter: &'a AsyncCanAdapter, config: IsoTPConfig) -> Self {
        Self { adapter, config }
    }

    fn pad(&self, data: &mut Vec<u8>) {
        if let Some(padding) = self.config.padding {
            let len = self.config.tx_dl - data.len();
            data.extend(std::iter::repeat(padding).take(len));
        }
    }

    pub async fn send_single_frame(&self, data: &[u8]) {
        let mut buf = vec![FrameType::Single as u8 | data.len() as u8];
        buf.extend(data);
        self.pad(&mut buf);

        debug!("TX SF, length: {} data {}", data.len(), hex::encode(&buf));

        let frame = Frame::new(self.config.bus, self.config.tx_id, &buf);
        self.adapter.send(&frame).await;
    }

    pub async fn send_first_frame(&self, data: &[u8]) {
        let b0: u8 = FrameType::First as u8 | ((data.len() >> 8) & 0xF) as u8;
        let b1: u8 = (data.len() & 0xFF) as u8;

        let mut buf = vec![b0, b1];
        buf.extend(&data[..self.config.tx_dl - 2]);

        debug!("TX FF, length: {} data {}", data.len(), hex::encode(&buf));

        let frame = Frame::new(self.config.bus, self.config.tx_id, &buf);
        self.adapter.send(&frame).await;
    }

    pub async fn send_consecutive_frame(&self, data: &[u8], idx: usize) {
        let idx = ((idx + 1) & 0xF) as u8;

        let mut buf = vec![FrameType::Consecutive as u8 | idx];
        buf.extend(data);
        self.pad(&mut buf);

        debug!("TX CF, idx: {} data {}", idx, hex::encode(&buf));

        let frame = Frame::new(self.config.bus, self.config.tx_id, &buf);
        self.adapter.send(&frame).await;
    }

    fn receive_flow_control(&self, frame: &Frame) -> Result<FlowControlConfig, Error> {
        // Check if Flow Control
        if FrameType::from_repr(frame.data[0] & FRAME_TYPE_MASK) != Some(FrameType::FlowControl) {
            return Err(crate::isotp::error::Error::FlowControl.into());
        };

        // Check Flow Status
        match FlowStatus::from_repr(frame.data[0] & FLOW_SATUS_MASK) {
            Some(FlowStatus::ContinueToSend) => {} // Ok
            Some(FlowStatus::Wait) => unimplemented!("Wait flow control not implemented"),
            Some(FlowStatus::Overflow) => return Err(crate::isotp::error::Error::Overflow.into()),
            None => return Err(crate::isotp::error::Error::MalformedFrame.into()),
        };

        // Parse block size and separation time
        let config = types::FlowControlConfig::try_from(frame)?;

        debug!("RX FC, {:?} data {}", config, hex::encode(&frame.data));
        Ok(config)
    }

    async fn send_multiple(&self, data: &[u8]) -> Result<(), Error> {
        // Stream for receiving flow control
        let stream = self
            .adapter
            .recv_filter(|frame| frame.id == self.config.rx_id && !frame.loopback)
            .timeout(self.config.timeout);
        tokio::pin!(stream);

        self.send_first_frame(data).await;
        let frame = stream.next().await.unwrap()?;
        let fc_config = self.receive_flow_control(&frame)?;

        debug!("RX FC, data {}", hex::encode(&frame.data));

        // Check for separation time override
        let st_min = match self.config.separation_time_min {
            Some(st_min) => st_min,
            None => fc_config.separation_time_min,
        };

        let chunks = data[self.config.tx_dl - 2..].chunks(self.config.tx_dl - 1);
        let mut it = chunks.enumerate().peekable();
        while let Some((idx, chunk)) = it.next() {
            self.send_consecutive_frame(chunk, idx).await;

            // Sleep for separation time between frames
            let last = it.peek().is_none();
            if !last {
                tokio::time::sleep(st_min).await;
            }
        }

        Ok(())
    }

    /// Asynchronously send an ISO-TP frame of up to 4095 bytes. Returns [`Error::Timeout`] if the ECU is not responding in time with flow control messages.
    pub async fn send(&self, data: &[u8]) -> Result<(), Error> {
        debug!("TX {}", hex::encode(data));

        if data.len() < self.config.tx_dl {
            self.send_single_frame(data).await;
        } else if data.len() <= 4095 {
            self.send_multiple(data).await?;
        } else {
            return Err(crate::isotp::error::Error::DataTooLarge.into());
        }

        Ok(())
    }
    async fn recv_single_frame(&self, frame: &Frame) -> Result<Vec<u8>, Error> {
        let len = (frame.data[0] & 0xF) as usize;
        if len == 0 {
            unimplemented!("CAN FD escape sequence for single frame not supported");
        }

        debug!("RX SF, length: {} data {}", len, hex::encode(&frame.data));

        Ok(frame.data[1..len + 1].to_vec())
    }

    async fn recv_first_frame(&self, frame: &Frame, buf: &mut Vec<u8>) -> Result<usize, Error> {
        let b0 = frame.data[0] as u16;
        let b1 = frame.data[1] as u16;
        let len = ((b0 << 8 | b1) & 0xFFF) as usize;

        debug!("RX FF, length: {}, data {}", len, hex::encode(&frame.data));
        if len == 0 {
            unimplemented!("CAN FD escape sequence for first frame not supported");
        }


        buf.extend(&frame.data[2..]);

        // Send Flow Control
        let mut flow_control = vec![0x30, 0x00, 0x00];
        self.pad(&mut flow_control);

        debug!("TX FC, data {}", hex::encode(&flow_control));

        let frame = Frame::new(self.config.bus, self.config.tx_id, &flow_control);
        self.adapter.send(&frame).await;

        Ok(len)
    }

    async fn recv_consecutive_frame(
        &self,
        frame: &Frame,
        buf: &mut Vec<u8>,
        len: usize,
        idx: u8,
    ) -> Result<u8, Error> {
        let msg_idx = frame.data[0] & 0xF;
        let remaining_len = len - buf.len();
        let end_idx = std::cmp::min(remaining_len + 1, frame.data.len());

        buf.extend(&frame.data[1..end_idx]);
        debug!(
            "RX CF, idx: {}, data {} {}",
            idx,
            hex::encode(&frame.data),
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
    ) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        let mut len: Option<usize> = None;
        let mut idx: u8 = 1;

        while let Some(frame) = stream.next().await {
            let frame = frame?;
            match FrameType::from_repr(frame.data[0] & FRAME_TYPE_MASK) {
                Some(FrameType::Single) => {
                    return self.recv_single_frame(&frame).await;
                }
                Some(FrameType::First) => {
                    // If we already received a first frame, something went wrong
                    if len.is_some() {
                        return Err(Error::IsoTPError(crate::isotp::error::Error::OutOfOrder));
                    }
                    len = Some(self.recv_first_frame(&frame, &mut buf).await?);
                }
                Some(FrameType::Consecutive) => {
                    if let Some(len) = len {
                        idx = self
                            .recv_consecutive_frame(&frame, &mut buf, len, idx)
                            .await?;
                        if buf.len() >= len {
                            return Ok(buf);
                        }
                    } else {
                        return Err(Error::IsoTPError(crate::isotp::error::Error::OutOfOrder));
                    }
                }
                Some(FrameType::FlowControl) => {} // Ignore flow control frames, these are from a simultaneous transmission
                _ => {
                    return Err(crate::isotp::error::Error::UnknownFrameType.into());
                }
            };
        }
        unreachable!();
    }

    /// Stream of ISO-TP packets. Can be used if multiple responses are expected from a single request. Returns [`Error::Timeout`] if the timeout is exceeded between individual ISO-TP frames. Note the total time to receive a packet may be longer than the timeout.
    pub fn recv(&self) -> impl Stream<Item = Result<Vec<u8>, Error>> + '_ {
        let stream = self
            .adapter
            .recv_filter(|frame| frame.id == self.config.rx_id && !frame.loopback)
            .timeout(self.config.timeout);

        Box::pin(stream! {
            tokio::pin!(stream);

            loop {
                yield self.recv_from_stream(&mut stream).await;
            }
        })
    }
}
