//! XCP on CAN and CAN FD, with one packet per data frame.

use std::{pin::Pin, sync::Arc};

use super::{PacketLimits, TransportFuture, XCPTransport};
use crate::{
    can::{AsyncCanAdapter, Frame, Identifier, DLC_TO_LEN},
    Result, Stream, StreamExt,
};

/// XCP CAN addressing and transmit framing. SocketCAN timing is configured by the OS.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct XCPCanConfig {
    pub tx_id: Identifier,
    pub rx_id: Identifier,
    pub bus: u8,
    pub fd: bool,
    pub brs: Option<bool>,
    pub padding: Option<u8>,
    /// None permits eight bytes for CAN or 64 for CAN FD.
    pub max_dlen: Option<usize>,
}

impl XCPCanConfig {
    pub fn new(tx_id: Identifier, rx_id: Identifier) -> Self {
        Self {
            tx_id,
            rx_id,
            bus: 0,
            fd: false,
            brs: None,
            padding: Some(0),
            max_dlen: None,
        }
    }
    pub fn capacity(&self) -> usize {
        self.max_dlen.unwrap_or(if self.fd { 64 } else { 8 })
    }
    pub fn validate(&self) -> Result<()> {
        let capacity = self.capacity();
        if self.tx_id == self.rx_id
            || (!self.fd && (capacity != 8 || self.brs == Some(true)))
            || (self.fd && ![8, 12, 16, 20, 24, 32, 48, 64].contains(&capacity))
        {
            return Err(crate::Error::InvalidAdapterConfig(
                "invalid XCP CAN identifiers, frame length or BRS setting".into(),
            ));
        }
        Ok(())
    }
}

pub struct XCPOnCAN {
    adapter: Arc<AsyncCanAdapter>,
    config: XCPCanConfig,
    receiver: Pin<Box<dyn Stream<Item = Frame> + Send>>,
}

impl XCPOnCAN {
    pub fn new(adapter: Arc<AsyncCanAdapter>, config: XCPCanConfig) -> Result<Self> {
        config.validate()?;
        let receiver = Box::pin(adapter.recv_filter(move |f| {
            !f.loopback
                && f.is_data()
                && f.bus == config.bus
                && f.id() == Some(config.rx_id)
                && f.is_fd() == config.fd
        }));
        Ok(Self {
            adapter,
            config,
            receiver,
        })
    }
}

impl XCPTransport for XCPOnCAN {
    fn limits(&self) -> PacketLimits {
        PacketLimits {
            max_tx_packet_size: self.config.capacity(),
            max_rx_packet_size: if self.config.fd { 64 } else { 8 },
            padded: true,
        }
    }
    fn send<'a>(&'a mut self, data: &'a [u8]) -> TransportFuture<'a, ()> {
        Box::pin(async move {
            if data.len() > self.config.capacity() {
                return Err(crate::Error::MalformedFrame);
            }
            let mut payload = data.to_vec();
            let target = if self.config.padding.is_some() {
                self.config.capacity()
            } else {
                data.len()
            };
            let length = if self.config.fd {
                DLC_TO_LEN
                    .iter()
                    .copied()
                    .find(|&n| n >= target)
                    .ok_or(crate::Error::MalformedFrame)?
            } else {
                target
            };
            payload.resize(length, self.config.padding.unwrap_or(0));
            let frame = Frame::builder(self.config.tx_id)
                .bus(self.config.bus)
                .fd(self.config.fd)
                .brs(
                    self.config.fd
                        && self
                            .config
                            .brs
                            .unwrap_or_else(|| self.adapter.default_brs()),
                )
                .data(&payload)
                .build()?;
            self.adapter.send(&frame).await
        })
    }
    fn recv(&mut self) -> TransportFuture<'_, Vec<u8>> {
        Box::pin(async move {
            self.receiver
                .next()
                .await
                .map(|f| f.data().to_vec())
                .ok_or(crate::Error::Disconnected)
        })
    }
}
