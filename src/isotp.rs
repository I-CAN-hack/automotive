use crate::async_can::AsyncCanAdapter;
use crate::can::Identifier;
use crate::can::Frame;
use crate::error::Error;

use futures_util::stream::StreamExt;

pub struct IsoTPConfig {
    bus: u8,
    tx_id: Identifier,
    rx_id: Identifier,

    tx_dl: usize,
    max_sf_dl: usize,
    padding: u8,
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
        }
    }
}


pub struct IsoTP<'a> {
    adapter: &'a AsyncCanAdapter,
    config: IsoTPConfig,
}

impl<'a> IsoTP<'a> {
    pub fn new(adapter: &'a AsyncCanAdapter, config: IsoTPConfig) -> Self {
        Self {
            adapter,
            config,
        }
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

    pub async fn recv(&self) -> Result<Vec<u8>, Error> {
        // TODO: Implement timeout
        // let rx_id = self.config.rx_id;
        let mut stream = self.adapter.recv_filter(|frame| frame.id == self.config.rx_id);

        while let Some(frame) = stream.next().await {
            return Ok(frame.data);
        }

        unreachable!()
    }
}
