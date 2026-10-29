//! Diagnostics over IP (ISO 13400-2) for [`crate::protocols::uds::UDSClient`].
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
mod discovery;
mod frame;

pub use config::{DoIPConfig, DoIPConfigBuilder};
pub use discovery::{
    discover, discover_with_config, Discovery, DiscoveryConfig, VehicleAnnouncement,
    VehicleIdentification,
};

use crate::{Result, Transport};
use frame::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{lookup_host, TcpSocket, TcpStream, ToSocketAddrs},
    sync::{broadcast, mpsc, oneshot, watch, Mutex},
    task::JoinHandle,
    time::{timeout, Instant},
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

struct Command {
    data: Vec<u8>,
    reply: oneshot::Sender<Result<()>>,
}

struct Inner {
    config: DoIPConfig,
    commands: mpsc::Sender<Command>,
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
        let mut stream = timeout(config.connect_timeout, async {
            let mut last_error = None;
            for address in lookup_host(address).await? {
                if config
                    .local_address
                    .is_some_and(|local| local.is_ipv4() != address.is_ipv4())
                {
                    continue;
                }
                let socket = if address.is_ipv4() {
                    TcpSocket::new_v4()?
                } else {
                    TcpSocket::new_v6()?
                };
                if let Some(local) = config.local_address {
                    socket.bind(local)?;
                }
                match socket.connect(address).await {
                    Ok(stream) => return Ok(stream),
                    Err(error) => last_error = Some(error),
                }
            }
            Err(last_error.unwrap_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::AddrNotAvailable,
                    "no matching DoIP endpoint address",
                )
            }))
        })
        .await
        .map_err(|_| crate::Error::Timeout)??;
        stream.set_nodelay(true)?;
        let peer_address = stream.peer_addr()?;
        let mut buffered = Vec::new();
        let entity_address = timeout(
            config.activation_timeout,
            activate(&mut stream, &mut buffered, &config),
        )
        .await
        .map_err(|_| crate::Error::Timeout)??;
        let (commands, receiver) = mpsc::channel(16);
        let (messages, _) = broadcast::channel(64);
        let (terminal, _) = watch::channel(None);
        let worker = tokio::spawn(run(
            stream,
            buffered,
            config.clone(),
            receiver,
            messages.clone(),
            terminal.clone(),
        ));
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
        let (reply, result) = oneshot::channel();
        self.inner
            .commands
            .send(Command {
                data: data.to_vec(),
                reply,
            })
            .await
            .map_err(|_| crate::Error::Disconnected)?;
        result.await.map_err(|_| crate::Error::Disconnected)?
    }

    fn recv(&self) -> impl crate::Stream<Item = Result<Vec<u8>>> + Unpin + '_ {
        self.recv_owned()
    }
}

async fn write(
    stream: &mut TcpStream,
    config: &DoIPConfig,
    kind: u16,
    payload: &[u8],
) -> Result<()> {
    timeout(
        config.ack_timeout,
        stream.write_all(&encode(config.protocol_version, kind, payload)),
    )
    .await
    .map_err(|_| crate::Error::Timeout)??;
    Ok(())
}

async fn activate(
    stream: &mut TcpStream,
    buffered: &mut Vec<u8>,
    config: &DoIPConfig,
) -> Result<u16> {
    let mut payload = config.source_address.to_be_bytes().to_vec();
    payload.push(config.activation_type);
    payload.extend([0; 4]);
    if let Some(oem) = config.activation_oem {
        payload.extend(oem);
    }
    write(stream, config, ACTIVATION_REQUEST, &payload).await?;
    loop {
        while let Some(message) = decode(buffered, config.max_payload_size)? {
            if message.version != config.protocol_version {
                return Err(invalid("TCP protocol version changed"));
            }
            match message.kind {
                ACTIVATION_RESPONSE => {
                    let payload = &message.payload;
                    if payload[..2] != config.source_address.to_be_bytes() {
                        return Err(invalid("routing activation tester address mismatch"));
                    }
                    if payload[4] != 0x10 {
                        return Err(Error::RoutingActivation(payload[4]).into());
                    }
                    return Ok(u16::from_be_bytes([payload[2], payload[3]]));
                }
                ALIVE_REQUEST => {
                    write(
                        stream,
                        config,
                        ALIVE_RESPONSE,
                        &config.source_address.to_be_bytes(),
                    )
                    .await?
                }
                HEADER_NACK => return Err(Error::HeaderNack(message.payload[0]).into()),
                _ => {}
            }
        }
        if stream.read_buf(buffered).await? == 0 {
            return Err(crate::Error::Disconnected);
        }
    }
}

struct Pending {
    command: Command,
    deadline: Instant,
}

async fn run(
    mut stream: TcpStream,
    mut buffered: Vec<u8>,
    config: DoIPConfig,
    mut commands: mpsc::Receiver<Command>,
    messages: broadcast::Sender<Result<Vec<u8>>>,
    terminal: watch::Sender<Option<crate::Error>>,
) {
    let mut pending: Option<Pending> = None;
    let outcome: Result<()> = async {
        loop {
            while let Some(message) = decode(&mut buffered, config.max_payload_size)? {
                if message.version != config.protocol_version { return Err(invalid("TCP protocol version changed")); }
                let payload = &message.payload;
                match message.kind {
                    ALIVE_REQUEST => write(&mut stream, &config, ALIVE_RESPONSE, &config.source_address.to_be_bytes()).await?,
                    DIAGNOSTIC => {
                        if payload[..2] == config.target_address.to_be_bytes() && payload[2..4] == config.source_address.to_be_bytes() {
                            let _ = messages.send(Ok(payload[4..].to_vec()));
                        }
                    }
                    ACK | NACK => {
                        let Some(active) = pending.as_ref() else { continue; };
                        if payload[..2] != config.target_address.to_be_bytes() || payload[2..4] != config.source_address.to_be_bytes() {
                            continue;
                        }
                        if !active.command.data.starts_with(&payload[5..]) { return Err(invalid("diagnostic ACK echo mismatch")); }
                        let result = if message.kind == NACK {
                            Err(Error::DiagnosticNack(payload[4]).into())
                        } else if payload[4] != 0 {
                            return Err(invalid("nonzero diagnostic ACK code"));
                        } else { Ok(()) };
                        let active = pending.take().unwrap();
                        let _ = active.command.reply.send(result);
                    }
                    HEADER_NACK => return Err(Error::HeaderNack(payload[0]).into()),
                    _ => {},
                }
            }
            let deadline = pending.as_ref().map(|active| active.deadline);
            tokio::select! {
                biased;
                _ = async { match deadline { Some(deadline) => tokio::time::sleep_until(deadline).await, None => std::future::pending().await } } => return Err(crate::Error::Timeout),
                read = stream.read_buf(&mut buffered) => {
                    if read? == 0 { return Err(crate::Error::Disconnected); }
                }
                command = commands.recv(), if pending.is_none() => {
                    let Some(command) = command else { return Ok(()); };
                    // A cancelled caller that never reached the wire needs no transmission.
                    if command.reply.is_closed() { continue; }
                    let mut payload = config.source_address.to_be_bytes().to_vec();
                    payload.extend(config.target_address.to_be_bytes());
                    payload.extend(&command.data);
                    pending = Some(Pending { command, deadline: Instant::now() + config.ack_timeout });
                    write(&mut stream, &config, DIAGNOSTIC, &payload).await?;
                    pending.as_mut().unwrap().deadline = Instant::now() + config.ack_timeout;
                }
            }
        }
    }.await;
    let error = outcome.err().unwrap_or(crate::Error::Disconnected);
    if let Some(active) = pending {
        let _ = active.command.reply.send(Err(error.clone()));
    }
    terminal.send_replace(Some(error));
}
