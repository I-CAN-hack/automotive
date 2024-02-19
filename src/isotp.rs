use crate::async_can::AsyncCanAdapter;
use crate::can::Frame;
use crate::can::Identifier;
use crate::error::Error;
use tracing::{info, warn};

use tokio_stream::StreamExt;

const DEFAULT_TIMEOUT_MS: u64 = 1000;

pub struct IsoTPConfig {
    bus: u8,
    tx_id: Identifier,
    rx_id: Identifier,

    tx_dl: usize,
    max_sf_dl: usize,
    padding: u8,
    timeout: std::time::Duration,
}

impl IsoTPConfig {
    pub fn new(bus: u8, id: Identifier) -> Self {
        let tx_id = id;
        let rx_id = match id {
            Identifier::Standard(id) => Identifier::Standard(id + 8),
            Identifier::Extended(_) => unimplemented!("Only standard IDs supported"),
        };

        Self {
            bus,
            tx_id,
            rx_id,

            // Message size config
            tx_dl: 8,
            max_sf_dl: 7, // 7 bytes with normal addressing, 6 bytes with extended addressing

            padding: 0xaa,

            timeout: std::time::Duration::from_millis(DEFAULT_TIMEOUT_MS),
        }
    }
}

pub struct IsoTP<'a> {
    adapter: &'a AsyncCanAdapter,
    config: IsoTPConfig,
}

impl<'a> IsoTP<'a> {
    pub fn new(adapter: &'a AsyncCanAdapter, config: IsoTPConfig) -> Self {
        Self { adapter, config }
    }

    pub async fn send_single_frame(&self, data: &[u8]) {
        if self.config.max_sf_dl > 7 {
            unimplemented!("CAN FD escape sequence for single frame not supported");
        }

        // Single Frame + Length
        let mut buf = vec![data.len() as u8];

        // Data
        buf.extend(data);

        // Pad to tx_dl
        buf.extend(std::iter::repeat(self.config.padding).take(self.config.tx_dl - buf.len()));

        info!("TX SF, length: {} data {}", data.len(), hex::encode(&buf));

        let frame = Frame::new(self.config.bus, self.config.tx_id, &buf);
        self.adapter.send(&frame).await;
    }

    pub async fn send(&self, data: &[u8]) -> Result<(), Error>{
        info!("TX {}", hex::encode(&data));

        if data.len() <= self.config.max_sf_dl {
            self.send_single_frame(data).await;
        } else {
            unimplemented!("Multi-frame ISO-TP not implemented");
        }

        Ok(())
    }

    async fn handle_single_frame(
        &self,
        frame: Frame,
        buf: &mut Vec<u8>,
        len: &mut usize,
    ) -> Result<(), Error> {
        *len = (frame.data[0] & 0xF) as usize;
        if *len == 0 {
            unimplemented!("CAN FD escape sequence for single frame not supported");
        }

        info!(
            "RX SF, length: {} data {}",
            *len,
            hex::encode(&frame.data[1..*len + 1])
        );

        buf.extend(&frame.data[1..*len + 1]);

        return Ok(());
    }

    async fn handle_first_frame(
        &self,
        frame: Frame,
        buf: &mut Vec<u8>,
        len: &mut usize,
    ) -> Result<(), Error> {
        // Length from byte 0 and 1
        let b0 = frame.data[0] as u16;
        let b1 = frame.data[1] as u16;
        *len = ((b0 << 8 | b1) & 0xFFF) as usize;

        info!(
            "RX FF, length: {}, data {}",
            *len,
            hex::encode(&frame.data[2..])
        );

        buf.extend(&frame.data[2..]);

        // Send Flow Control
        // TODO: pad to tx_dl?
        let flow_control = [0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let frame = Frame::new(self.config.bus, self.config.tx_id, &flow_control);
        self.adapter.send(&frame).await;

        return Ok(());
    }

    async fn handle_consecutive_frame(
        &self,
        frame: Frame,
        buf: &mut Vec<u8>,
        len: &mut usize,
        idx: &mut u8,
    ) -> Result<(), Error> {
        let msg_idx = (frame.data[0] & 0xF) as u8;
        let remaining_len = *len - buf.len();
        let end_idx = std::cmp::min(remaining_len + 1, frame.data.len() - 1);

        info!(
            "RX CF, idx: {}, data {}",
            idx,
            hex::encode(&frame.data[1..end_idx])
        );
        buf.extend(&frame.data[1..end_idx]);

        if msg_idx != *idx {
            warn!("ISO-TP multi-frame out of order");
        }

        *idx = if *idx == 0xF { 0 } else { *idx + 1 };

        return Ok(());
    }

    pub async fn recv(&self) -> Result<Vec<u8>, Error> {
        let stream = self
            .adapter
            .recv_filter(|frame| frame.id == self.config.rx_id)
            .timeout(self.config.timeout);
        tokio::pin!(stream);

        let mut buf = Vec::new();
        let mut len: usize = 0;
        let mut idx: u8 = 1;

        while let Some(frame) = stream.next().await {
            match frame {
                Ok(frame) => {
                    match (frame.data[0] & 0xF0) >> 4 {
                        0x0 => self.handle_single_frame(frame, &mut buf, &mut len).await?,
                        0x1 => self.handle_first_frame(frame, &mut buf, &mut len).await?,
                        0x2 => {
                            self.handle_consecutive_frame(frame, &mut buf, &mut len, &mut idx)
                                .await?
                        }
                        _ => {
                            unimplemented!("Unhandeled ISO-TP frame type {:x}", frame.data[0] >> 4)
                        }
                    };
                }
                Err(_) => {
                    return Err(Error::Timeout);
                }
            };

            if buf.len() >= len {
                break;
            }
        }
        info!("RX {}", hex::encode(&buf));
        Ok(buf)
    }
}
