//! CCP framing on Classical CAN (CCP 2.1 section 7).

use std::{future::Future, pin::Pin, sync::Arc};

use crate::{
    can::{AsyncCanAdapter, Frame, Identifier},
    Result, Stream, StreamExt,
};

pub type TransportFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// An exclusive CCP packet transport. CROs and CRMs are exactly eight bytes.
/// Receivers must retain packet boundaries across cancellation.
pub trait CCPTransport: Send {
    fn send<'a>(&'a mut self, data: &'a [u8; 8]) -> TransportFuture<'a, ()>;
    fn recv(&mut self) -> TransportFuture<'_, Vec<u8>>;
}

impl<T: CCPTransport + ?Sized> CCPTransport for Box<T> {
    fn send<'a>(&'a mut self, data: &'a [u8; 8]) -> TransportFuture<'a, ()> {
        (**self).send(data)
    }
    fn recv(&mut self) -> TransportFuture<'_, Vec<u8>> {
        (**self).recv()
    }
}

/// CCP CRO/DTO CAN identifiers and adapter bus. Supports 11-bit and 29-bit IDs.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CCPCanConfig {
    pub tx_id: Identifier,
    pub rx_id: Identifier,
    pub bus: u8,
}

impl CCPCanConfig {
    pub fn new(tx_id: Identifier, rx_id: Identifier) -> Self {
        Self {
            tx_id,
            rx_id,
            bus: 0,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.tx_id == self.rx_id {
            return Err(crate::Error::InvalidAdapterConfig(
                "CCP transmit and receive identifiers must differ".into(),
            ));
        }
        Ok(())
    }
}

/// Retains the adapter and subscribes before the first command is sent.
pub struct CCPOnCAN {
    adapter: Arc<AsyncCanAdapter>,
    config: CCPCanConfig,
    receiver: Pin<Box<dyn Stream<Item = Frame> + Send>>,
}

impl CCPOnCAN {
    pub fn new(adapter: Arc<AsyncCanAdapter>, config: CCPCanConfig) -> Result<Self> {
        config.validate()?;
        let receiver = Box::pin(adapter.recv_filter(move |f| {
            !f.loopback
                && f.is_data()
                && !f.is_fd()
                && f.bus == config.bus
                && f.id() == Some(config.rx_id)
        }));
        Ok(Self {
            adapter,
            config,
            receiver,
        })
    }
}

impl CCPTransport for CCPOnCAN {
    fn send<'a>(&'a mut self, data: &'a [u8; 8]) -> TransportFuture<'a, ()> {
        Box::pin(async move {
            self.adapter
                .send(
                    &Frame::builder(self.config.tx_id)
                        .bus(self.config.bus)
                        .data(data)
                        .build()?,
                )
                .await
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
