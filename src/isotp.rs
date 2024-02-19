use crate::async_can::AsyncCanAdapter;
use crate::can::Frame;
use crate::can::Identifier;
use crate::error::Error;

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

        let frame = Frame::new(self.config.bus, self.config.tx_id, &buf);
        self.adapter.send(&frame).await;
    }

    pub async fn send(&self, data: &[u8]) {
        if data.len() <= self.config.max_sf_dl {
            return self.send_single_frame(data).await;
        }

        unimplemented!("Multi-frame ISO-TP not implemented");
    }

    fn handle_single_frame(
        &self,
        frame: Frame,
        buf: &mut Vec<u8>,
        len: &mut usize,
    ) -> Result<(), Error> {
        *len = (frame.data[0] & 0xF) as usize;
        if *len == 0 {
            unimplemented!("CAN FD escape sequence for single frame not supported");
        }

        buf.extend(&frame.data[1..*len + 1]);

        return Ok(());
    }

    pub async fn recv(&self) -> Result<Vec<u8>, Error> {
        let stream = self
            .adapter
            .recv_filter(|frame| frame.id == self.config.rx_id);

        let stream = stream.timeout(self.config.timeout);
        tokio::pin!(stream);

        let mut buf = Vec::new();
        let mut len: usize = 0;

        while let Some(frame) = stream.next().await {
            match frame {
                Ok(frame) => {
                    match (frame.data[0] & 0xF0) >> 4 {
                        0x0 => self.handle_single_frame(frame, &mut buf, &mut len)?,
                        _ => unimplemented!("Unhandeled ISO-TP frame type"),
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

        Ok(buf)
    }
}
