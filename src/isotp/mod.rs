mod constants;
pub mod error;

use crate::async_can::AsyncCanAdapter;
use crate::can::Frame;
use crate::can::Identifier;
use crate::error::Error;
use crate::isotp::constants::FrameType;
use tokio_stream::StreamExt;
use tracing::debug;

const DEFAULT_TIMEOUT_MS: u64 = 100;

pub struct IsoTPConfig {
    bus: u8,
    tx_id: Identifier,
    rx_id: Identifier,

    tx_dl: usize,
    padding: u8,
    timeout: std::time::Duration,
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

        Self {
            bus,
            tx_id,
            rx_id,
            tx_dl: 8,
            padding: 0xaa,
            timeout: std::time::Duration::from_millis(DEFAULT_TIMEOUT_MS),
        }
    }
}

pub struct IsoTPAdapter<'a> {
    adapter: &'a AsyncCanAdapter,
    config: IsoTPConfig,
}

impl<'a> IsoTPAdapter<'a> {
    pub fn from_id(adapter: &'a AsyncCanAdapter, id: u32) -> Self {
        let config = IsoTPConfig::new(0, id.into());
        Self::new(adapter, config)
    }

    pub fn new(adapter: &'a AsyncCanAdapter, config: IsoTPConfig) -> Self {
        Self { adapter, config }
    }

    fn pad(&self, data: &mut Vec<u8>) {
        let len = self.config.tx_dl - data.len();
        data.extend(std::iter::repeat(self.config.padding).take(len));
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

    pub async fn send_multiple(&self, data: &[u8]) -> Result<(), Error> {
        // Stream for receiving flow control
        let stream = self
            .adapter
            .recv_filter(|frame| frame.id == self.config.rx_id && !frame.returned)
            .timeout(self.config.timeout);
        tokio::pin!(stream);

        self.send_first_frame(data).await;
        let frame = stream.next().await.unwrap()?;
        if frame.data[0] & 0xF0 != FrameType::FlowControl as u8 {
            return Err(Error::IsoTPError(crate::isotp::error::Error::FlowControl));
        };
        debug!("RX FC, data {}", hex::encode(&frame.data));

        let chunks = data[self.config.tx_dl - 2..].chunks(self.config.tx_dl - 1);
        for (idx, chunk) in chunks.enumerate() {
            self.send_consecutive_frame(chunk, idx).await;
        }

        Ok(())
    }

    pub async fn send(&self, data: &[u8]) -> Result<(), Error> {
        debug!("TX {}", hex::encode(&data));

        if data.len() <= self.config.tx_dl - 1 {
            self.send_single_frame(data).await;
        } else if data.len() <= 4095 {
            self.send_multiple(data).await?;
        } else {
            return Err(Error::IsoTPError(crate::isotp::error::Error::DataTooLarge));
        }

        Ok(())
    }

    async fn recv_single_frame(
        &self,
        frame: Frame,
        buf: &mut Vec<u8>,
        len: &mut usize,
    ) -> Result<(), Error> {
        *len = (frame.data[0] & 0xF) as usize;
        if *len == 0 {
            // unimplemented!("CAN FD escape sequence for single frame not supported");
            return Err(Error::IsoTPError(
                crate::isotp::error::Error::MalformedFrame,
            ));
        }

        debug!("RX SF, length: {} data {}", *len, hex::encode(&frame.data));

        buf.extend(&frame.data[1..*len + 1]);

        return Ok(());
    }

    async fn recv_first_frame(
        &self,
        frame: Frame,
        buf: &mut Vec<u8>,
        len: &mut usize,
    ) -> Result<(), Error> {
        let b0 = frame.data[0] as u16;
        let b1 = frame.data[1] as u16;
        *len = ((b0 << 8 | b1) & 0xFFF) as usize;

        debug!("RX FF, length: {}, data {}", *len, hex::encode(&frame.data));

        buf.extend(&frame.data[2..]);

        // Send Flow Control
        let mut flow_control = vec![0x30, 0x00, 0x00];
        self.pad(&mut flow_control);

        debug!("TX FC, data {}", hex::encode(&flow_control));

        let frame = Frame::new(self.config.bus, self.config.tx_id, &flow_control);
        self.adapter.send(&frame).await;

        return Ok(());
    }

    async fn recv_consecutive_frame(
        &self,
        frame: Frame,
        buf: &mut Vec<u8>,
        len: &mut usize,
        idx: &mut u8,
    ) -> Result<(), Error> {
        let msg_idx = (frame.data[0] & 0xF) as u8;
        let remaining_len = *len - buf.len();
        let end_idx = std::cmp::min(remaining_len + 1, frame.data.len());

        buf.extend(&frame.data[1..end_idx]);
        debug!(
            "RX CF, idx: {}, data {} {}",
            idx,
            hex::encode(&frame.data),
            hex::encode(&buf)
        );

        if msg_idx != *idx {
            return Err(Error::IsoTPError(crate::isotp::error::Error::OutOfOrder));
        }

        *idx = if *idx == 0xF { 0 } else { *idx + 1 };

        return Ok(());
    }

    pub async fn recv(&self) -> Result<Vec<u8>, Error> {
        let stream = self
            .adapter
            .recv_filter(|frame| frame.id == self.config.rx_id && !frame.returned)
            .timeout(self.config.timeout);
        tokio::pin!(stream);

        let mut buf = Vec::new();
        let mut len: usize = 0;
        let mut idx: u8 = 1;

        while let Some(frame) = stream.next().await {
            let frame = frame?;
            match (frame.data[0] & 0xF0).into() {
                FrameType::Single => self.recv_single_frame(frame, &mut buf, &mut len).await?,
                FrameType::First => self.recv_first_frame(frame, &mut buf, &mut len).await?,
                FrameType::Consecutive => {
                    self.recv_consecutive_frame(frame, &mut buf, &mut len, &mut idx)
                        .await?
                }
                _ => {
                    return Err(Error::IsoTPError(
                        crate::isotp::error::Error::UnknownFrameType,
                    ));
                }
            };

            debug!("{} {}", len, buf.len());

            if buf.len() >= len {
                break;
            }
        }
        debug!("RX {}", hex::encode(&buf));
        Ok(buf)
    }
}
