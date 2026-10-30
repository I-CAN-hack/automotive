//! TCP connection setup, routing activation, and the active connection worker
//! (ISO 13400-2:2025 §12.5, §9.5, and §9.6).
//! The worker owns the socket so cancellation of public operations cannot lose bytes.

use super::{
    frame::{self, invalid, AcknowledgementCode, LogicalAddresses, Payload, Request},
    DoIPConfig, Error,
};
use crate::Result;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{lookup_host, TcpSocket, TcpStream, ToSocketAddrs},
    sync::{broadcast, mpsc, oneshot, watch},
    time::{timeout, Instant},
};

pub(super) async fn connect_tcp(
    address: impl ToSocketAddrs,
    config: &DoIPConfig,
) -> Result<TcpStream> {
    let stream = timeout(config.connect_timeout, async {
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
    Ok(stream)
}

/// Routing activation registers the tester's logical address on the TCP socket.
/// Diagnostic messages are routed only after the entity accepts that registration.
pub(super) async fn activate_routing(
    stream: &mut TcpStream,
    buffered: &mut Vec<u8>,
    config: &DoIPConfig,
) -> Result<u16> {
    write_request(
        stream,
        config,
        Request::RoutingActivation {
            source_address: config.source_address,
            activation_type: config.activation_type,
            oem: config.activation_oem,
        },
    )
    .await?;
    loop {
        while let Some(payload) = decode_next_payload(buffered, config)? {
            match payload {
                Payload::RoutingActivation {
                    tester_address,
                    entity_address,
                    code,
                } => {
                    // ISO 13400-2:2025 §12.5, Tables 55–56: the response echoes
                    // the tester address; only 0x10 completes activation here.
                    // TLS/authentication/confirmation codes remain caller-visible.
                    if tester_address != config.source_address {
                        return Err(invalid("routing activation tester address mismatch"));
                    }
                    if code != 0x10 {
                        return Err(Error::RoutingActivation(code).into());
                    }
                    return Ok(entity_address);
                }
                Payload::AliveCheckRequest => answer_alive_check(stream, config).await?,
                Payload::HeaderNack(code) => return Err(Error::HeaderNack(code).into()),
                _ => {}
            }
        }
        read_into_buffer(stream, buffered).await?;
    }
}

pub(super) struct SendRequest {
    pub data: Vec<u8>,
    pub completion: oneshot::Sender<Result<()>>,
}

/// A transmitted request remains pending until its DoIP ACK/NACK arrives, even
/// if the caller cancels its wait. This prevents that ACK completing a later send.
struct PendingSend {
    request: SendRequest,
    deadline: Instant,
}

/// At most one outgoing diagnostic message awaits a DoIP ACK/NACK at a time.
/// Incoming UDS replies and alive checks remain independent of that acknowledgement.
pub(super) struct Connection {
    stream: TcpStream,
    buffered: Vec<u8>,
    config: DoIPConfig,
    pending: Option<PendingSend>,
}

impl Connection {
    pub fn new(stream: TcpStream, buffered: Vec<u8>, config: DoIPConfig) -> Self {
        Self {
            stream,
            buffered,
            config,
            pending: None,
        }
    }

    pub async fn run(
        mut self,
        mut commands: mpsc::Receiver<SendRequest>,
        messages: broadcast::Sender<Result<Vec<u8>>>,
        terminal: watch::Sender<Option<crate::Error>>,
    ) {
        let error = self
            .run_until_closed(&mut commands, &messages)
            .await
            .err()
            .unwrap_or(crate::Error::Disconnected);
        if let Some(pending) = self.pending {
            let _ = pending.request.completion.send(Err(error.clone()));
        }
        terminal.send_replace(Some(error));
    }

    async fn run_until_closed(
        &mut self,
        commands: &mut mpsc::Receiver<SendRequest>,
        messages: &broadcast::Sender<Result<Vec<u8>>>,
    ) -> Result<()> {
        loop {
            // Drain complete buffered frames before waiting for more input. A UDS
            // response may precede its DoIP ACK or share a read with that ACK.
            while let Some(payload) = decode_next_payload(&mut self.buffered, &self.config)? {
                self.handle_payload(payload, messages).await?;
            }

            let deadline = self.pending.as_ref().map(|pending| pending.deadline);
            tokio::select! {
                biased;
                _ = wait_for_ack_deadline(deadline) => return Err(crate::Error::Timeout),
                result = read_into_buffer(&mut self.stream, &mut self.buffered) => result?,
                request = commands.recv(), if self.pending.is_none() => {
                    let Some(request) = request else { return Ok(()); };
                    self.send_diagnostic(request).await?;
                }
            }
        }
    }

    async fn handle_payload(
        &mut self,
        payload: Payload,
        messages: &broadcast::Sender<Result<Vec<u8>>>,
    ) -> Result<()> {
        match payload {
            // §9.6 and §12.6.4: alive checks belong to socket maintenance and
            // must be answered even when no diagnostic operation is being polled.
            Payload::AliveCheckRequest => {
                answer_alive_check(&mut self.stream, &self.config).await?
            }
            Payload::Diagnostic { addresses, data } => {
                if addresses == self.response_addresses() {
                    // §9.5: vehicle-to-client diagnostic messages need no DoIP ACK.
                    let _ = messages.send(Ok(data));
                }
            }
            Payload::DiagnosticAcknowledgement {
                addresses,
                code,
                previous_data,
            } => self.acknowledge(addresses, code, &previous_data)?,
            Payload::HeaderNack(code) => return Err(Error::HeaderNack(code).into()),
            _ => {}
        }
        Ok(())
    }

    fn acknowledge(
        &mut self,
        addresses: LogicalAddresses,
        code: AcknowledgementCode,
        previous_data: &[u8],
    ) -> Result<()> {
        let Some(pending) = self.pending.as_ref() else {
            return Ok(());
        };
        if addresses != self.response_addresses() {
            return Ok(());
        }
        // §9.5, Tables 23 and 25: previous-message data is optional and may be
        // only a prefix. It must match the request whose ACK we are awaiting.
        if !pending.request.data.starts_with(previous_data) {
            return Err(invalid("diagnostic ACK echo mismatch"));
        }
        let result = match code {
            // §9.5, Table 24: zero confirms routing, independently of any UDS reply.
            AcknowledgementCode::Positive(0) => Ok(()),
            AcknowledgementCode::Positive(_) => return Err(invalid("nonzero diagnostic ACK code")),
            AcknowledgementCode::Negative(code) => Err(Error::DiagnosticNack(code).into()),
        };
        if let Some(pending) = self.pending.take() {
            let _ = pending.request.completion.send(result);
        }
        Ok(())
    }

    async fn send_diagnostic(&mut self, request: SendRequest) -> Result<()> {
        // A caller cancelled before transmission needs no request on the wire.
        if request.completion.is_closed() {
            return Ok(());
        }
        let message = Request::Diagnostic {
            addresses: LogicalAddresses {
                source: self.config.source_address,
                target: self.config.target_address,
            },
            data: &request.data,
        };
        let result = write_request(&mut self.stream, &self.config, message).await;

        // §7.7, Table 12: A_DoIP_Diagnostic_Message starts after transmission.
        // A timed-out connection is closed; this implementation never replays it.
        self.pending = Some(PendingSend {
            request,
            deadline: Instant::now() + self.config.ack_timeout,
        });
        // Write failures reach the completion channel with their original error.
        result
    }

    fn response_addresses(&self) -> LogicalAddresses {
        LogicalAddresses {
            source: self.config.target_address,
            target: self.config.source_address,
        }
    }
}

fn decode_next_payload(buffered: &mut Vec<u8>, config: &DoIPConfig) -> Result<Option<Payload>> {
    let Some(message) = frame::decode(buffered, config.max_payload_size)? else {
        return Ok(None);
    };
    // §9.2, 7.DoIP-195: responses use the entity's selected protocol version.
    if message.version != config.protocol_version {
        return Err(invalid("TCP protocol version changed"));
    }
    Ok(Some(message.payload))
}

async fn read_into_buffer(stream: &mut TcpStream, buffered: &mut Vec<u8>) -> Result<()> {
    if stream.read_buf(buffered).await? == 0 {
        return Err(crate::Error::Disconnected);
    }
    Ok(())
}

async fn write_request(
    stream: &mut TcpStream,
    config: &DoIPConfig,
    request: Request<'_>,
) -> Result<()> {
    // Socket writes and ACK waits each use the configured ack_timeout.
    // A partially written frame makes the connection unusable after a timeout.
    timeout(
        config.ack_timeout,
        stream.write_all(&request.encode(config.protocol_version)),
    )
    .await
    .map_err(|_| crate::Error::Timeout)??;
    Ok(())
}

async fn answer_alive_check(stream: &mut TcpStream, config: &DoIPConfig) -> Result<()> {
    write_request(
        stream,
        config,
        Request::AliveCheckResponse {
            source_address: config.source_address,
        },
    )
    .await
}

async fn wait_for_ack_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
