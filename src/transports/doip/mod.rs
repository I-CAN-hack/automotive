//! Diagnostics over IP (ISO 13400-2) for [`crate::protocols::uds::UDSClient`].
//!
//! A vehicle-side DoIP entity can be an ECU or a gateway to several ECUs. Its IP
//! address identifies the network endpoint; a logical address selects the ECU
//! receiving diagnostic messages. Routing activation registers the tester's
//! logical address on a TCP connection before diagnostics can be routed.
//!
//! [`discover`] actively identifies vehicles over IPv4 with no configuration.
//! [`VehicleAnnouncement::connect`] connects to a discovered entity. For a known
//! endpoint use [`DoIP::connect`]. [`Discovery`] also supports passive announcements,
//! VIN/EID filters, unicast, and scoped IPv6 multicast; see [`DiscoveryConfig`].
//!
//! Routing activation completes before connecting returns. A background task
//! frames TCP messages and answers alive checks even while no receiver is polled.
//! `send` completes on a matching DoIP ACK, independently of the UDS response.
//! Receive subscriptions must be created before sending. Serialize UDS exchanges
//! on a connection: the wire protocol has no transaction IDs. Reconnect after a
//! transport timeout, malformed TCP message, or cancellation of an exchange whose
//! UDS response is still outstanding. Requests are never automatically replayed.
//!
//! Supports unsecured TCP and protocol versions 0x02–0x04. TLS, OEM authentication,
//! and periodic diagnostic payload 0x8004 are outside this implementation.

mod config;
mod connection;
mod discovery;
mod frame;

pub use config::{DoIPConfig, DoIPConfigBuilder};
pub use discovery::{
    discover, discover_with_config, Discovery, DiscoveryConfig, VehicleAnnouncement,
    VehicleIdentification,
};

use crate::{Result, Transport};
use connection::{Connection, SendRequest};
use frame::invalid;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    net::ToSocketAddrs,
    sync::{broadcast, mpsc, oneshot, watch, Mutex},
    task::JoinHandle,
    time::timeout,
};

pub const DOIP_PORT: u16 = 13400;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid DoIP configuration: {0}")]
    InvalidConfig(String),
    #[error("Malformed DoIP message: {0}")]
    InvalidMessage(String),
    #[error("DoIP payload exceeds configured size limit")]
    PayloadTooLarge,
    #[error("DoIP routing activation failed: 0x{0:02x}")]
    RoutingActivation(u8),
    #[error("DoIP generic header NACK: 0x{0:02x}")]
    HeaderNack(u8),
    #[error("DoIP diagnostic NACK: 0x{0:02x}")]
    DiagnosticNack(u8),
    #[error("DoIP receive subscription lost messages")]
    ReceiveOverflow,
}

struct Inner {
    config: DoIPConfig,
    commands: mpsc::Sender<SendRequest>,
    messages: broadcast::Sender<Result<Vec<u8>>>,
    terminal: watch::Sender<Option<crate::Error>>,
    worker: JoinHandle<()>,
    send_gate: Mutex<()>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

/// A shared TCP connection to one physical ECU logical address.
/// Clones share the connection. Dropping the last clone closes the socket.
#[derive(Clone)]
pub struct DoIP {
    inner: Arc<Inner>,
    peer_address: SocketAddr,
    entity_address: u16,
}

impl DoIP {
    /// Connect with default tester address 0x0e00 and default routing activation.
    /// `address` includes a port, normally [`DOIP_PORT`]. OS routing chooses the
    /// adapter; IPv6 link-local endpoints require a scope ID (interface index).
    pub async fn connect(address: impl ToSocketAddrs, target_address: u16) -> Result<Self> {
        Self::connect_with_config(address, DoIPConfig::new(target_address)).await
    }

    pub async fn connect_with_config(
        address: impl ToSocketAddrs,
        config: DoIPConfig,
    ) -> Result<Self> {
        config.validate()?;
        let mut stream = connection::connect_tcp(address, &config).await?;
        let peer_address = stream.peer_addr()?;
        let mut buffered = Vec::new();

        // ISO 13400-2:2025 §12.5: activate routing before accepting diagnostic
        // sends. §7.7, Table 12 defines A_DoIP_Routing_Activation.
        let entity_address = timeout(
            config.activation_timeout,
            connection::activate_routing(&mut stream, &mut buffered, &config),
        )
        .await
        .map_err(|_| crate::Error::Timeout)??;
        let (commands, receiver) = mpsc::channel(16);
        let (messages, _) = broadcast::channel(64);
        let (terminal, _) = watch::channel(None);
        // The worker owns the socket and partial frames. After a caller cancels,
        // it still waits for the transmitted request's acknowledgement before
        // transmitting another request.
        let connection = Connection::new(stream, buffered, config.clone());
        let worker = tokio::spawn(connection.run(receiver, messages.clone(), terminal.clone()));
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                commands,
                messages,
                terminal,
                worker,
                send_gate: Mutex::new(()),
            }),
            peer_address,
            entity_address,
        })
    }

    pub fn config(&self) -> &DoIPConfig {
        &self.inner.config
    }

    pub fn peer_address(&self) -> SocketAddr {
        self.peer_address
    }

    /// Gateway address from routing activation; it may differ from the target ECU.
    pub fn entity_address(&self) -> u16 {
        self.entity_address
    }

    /// Close this connection and all its clones and wake waiting receivers.
    pub fn close(&self) {
        self.inner
            .terminal
            .send_replace(Some(crate::Error::Disconnected));
        self.inner.worker.abort();
    }

    /// Create an owned subscription immediately, suitable for a background consumer.
    /// Slow subscribers get [`Error::ReceiveOverflow`] rather than silent data loss.
    pub fn recv_owned(
        &self,
    ) -> impl crate::Stream<Item = Result<Vec<u8>>> + Send + Unpin + 'static {
        let mut receiver = self.inner.messages.subscribe();
        let mut terminal = self.inner.terminal.subscribe();
        Box::pin(async_stream::stream! {
            loop {
                let error = terminal.borrow().clone();
                if let Some(error) = error {
                    // Deliver already-buffered complete PDUs before a subsequent EOF.
                    match receiver.try_recv() {
                        Ok(message) => { yield message; continue; }
                        Err(broadcast::error::TryRecvError::Lagged(_)) => {
                            yield Err(Error::ReceiveOverflow.into());
                        }
                        _ => yield Err(error),
                    }
                    break;
                }
                tokio::select! {
                    biased;
                    message = receiver.recv() => match message {
                        Ok(message) => yield message,
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            yield Err(Error::ReceiveOverflow.into());
                            break;
                        }
                        Err(_) => break,
                    },
                    _ = terminal.changed() => {},
                }
            }
        })
    }
}

impl Transport for DoIP {
    async fn send<'a>(&'a self, data: &'a [u8]) -> Result<()> {
        if data.is_empty() {
            return Err(invalid("empty diagnostic PDU"));
        }
        if data.len() > self.inner.config.max_payload_size - 4 {
            return Err(Error::PayloadTooLarge.into());
        }
        let _guard = self.inner.send_gate.lock().await;
        if let Some(error) = self.inner.terminal.borrow().clone() {
            return Err(error);
        }
        let (completion, result) = oneshot::channel();
        self.inner
            .commands
            .send(SendRequest {
                data: data.to_vec(),
                completion,
            })
            .await
            .map_err(|_| crate::Error::Disconnected)?;
        result.await.map_err(|_| crate::Error::Disconnected)?
    }

    fn recv(&self) -> impl crate::Stream<Item = Result<Vec<u8>>> + Unpin + '_ {
        self.recv_owned()
    }
}
