//! XCP command client for Classical CAN, CAN FD, TCP/IP, and UDP/IP.
//!
//! Wraps individual ASAM XCP 1.5 commands (sections 7.5.1 and 7.5.2),
//! CAN transport framing, and Ethernet LEN/CTR framing. Each command call sends
//! one request. Callers handle transfer loops using the CONNECT capabilities.
//!
//! One client serializes its command exchanges.
//! Cancelling an active future lets its bounded exchange finish before another
//! operation starts. A timeout or malformed response requires `synchronize()`;
//! an interrupted TCP write requires a new client. Commands are never retried.
//! Optional commands remain subject to ECU support and resource protection.

mod constants;
mod error;
pub mod transport;
mod types;

pub use constants::*;
pub use error::{Error, ErrorCode};
pub use transport::{PacketLimits, XCPCanConfig, XCPOnCAN, XCPOnTCP, XCPOnUDP, XCPTransport};
pub use types::*;

use std::{future::Future, pin::Pin, sync::Arc};

use tokio::sync::{oneshot, watch, Mutex};

use crate::Result;
use error::malformed;

type Operation<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// An XCP client retaining its CAN adapter or Ethernet socket.
#[derive(Clone)]
pub struct XCPClient {
    inner: Arc<Mutex<Session>>,
    closed: watch::Sender<bool>,
}

impl XCPClient {
    /// Construct a client over any packet transport with default command timing.
    pub fn new(transport: impl XCPTransport + 'static) -> Result<Self> {
        Self::with_config(transport, XCPConfig::default())
    }

    pub fn with_config(transport: impl XCPTransport + 'static, config: XCPConfig) -> Result<Self> {
        validate_config(config)?;
        let limits = transport.limits();
        if limits.max_tx_packet_size < 8 || limits.max_rx_packet_size < 8 {
            return Err(Error::InvalidArgument(
                "XCP requires packet capacities of at least eight bytes".into(),
            )
            .into());
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(Session {
                link: Some(Box::new(transport)),
                config,
                info: None,
                desynchronized: false,
            })),
            closed: watch::channel(false).0,
        })
    }

    async fn run<T: Send + 'static>(
        &self,
        operation: impl for<'s> FnOnce(&'s mut Session) -> Operation<'s, T> + Send + 'static,
    ) -> Result<T> {
        if self.is_closed() {
            return Err(crate::Error::Disconnected);
        }
        let inner = self.inner.clone();
        let mut closed = self.closed.subscribe();
        let (mut result, response) = oneshot::channel();
        // Queued cancelled operations never transmit. Active operations drain
        // under the gate, because XCP positive responses do not echo a command ID.
        tokio::spawn(async move {
            let mut session = tokio::select! {
                biased;
                _ = result.closed() => return,
                _ = closed.wait_for(|value| *value) => { let _ = result.send(Err(crate::Error::Disconnected)); return; },
                session = inner.lock() => session,
            };
            if result.is_closed() {
                return;
            }
            let value = tokio::select! {
                biased;
                _ = closed.wait_for(|value| *value) => Err(crate::Error::Disconnected),
                value = operation(&mut session) => value,
            };
            let _ = result.send(value);
        });
        response.await.map_err(|_| crate::Error::Disconnected)?
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Release this client's transport and interrupt pending operations.
    /// Does not transmit DISCONNECT; call `disconnect` first for a graceful exit.
    pub async fn close(&self) {
        self.closed.send_replace(true);
        let mut session = self.inner.lock().await;
        session.link.take();
        session.info = None;
    }

    pub async fn connection_info(&self) -> Option<ConnectInfo> {
        self.inner.lock().await.info
    }

    pub async fn connect(&self, mode: impl Into<ConnectMode>) -> Result<ConnectInfo> {
        let mode = mode.into().as_raw();
        self.run(move |s| Box::pin(s.connect(mode))).await
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.run(|s| {
            Box::pin(async move {
                s.exchange(Command::Disconnect, &[]).await?;
                s.info = None;
                Ok(())
            })
        })
        .await
    }

    /// Recover command synchronization. SYNCH succeeds with ERR_CMD_SYNCH (0x00).
    pub async fn synchronize(&self) -> Result<()> {
        self.run(|s| Box::pin(async move { s.exchange(Command::Synch, &[]).await.map(|_| ()) }))
            .await
    }

    pub async fn get_status(&self) -> Result<Status> {
        self.run(|s| {
            Box::pin(async move {
                let order = s.connected()?.byte_order;
                let response = s.exchange(Command::GetStatus, &[]).await?;
                s.require(&response, 6)?;
                Ok(Status {
                    session_status: response[1],
                    resource_protection_status: response[2],
                    session_configuration_id: order.u16(&response[4..6]),
                })
            })
        })
        .await
    }

    pub async fn get_comm_mode_info(&self) -> Result<CommunicationModeInfo> {
        self.run(|s| {
            Box::pin(async move {
                let r = s.exchange(Command::GetCommModeInfo, &[]).await?;
                s.require(&r, 8)?;
                Ok(CommunicationModeInfo {
                    comm_mode_optional: r[2],
                    max_bs: r[4],
                    min_st: r[5],
                    queue_size: r[6],
                    driver_version: r[7],
                })
            })
        })
        .await
    }

    pub async fn get_id(&self, id_type: impl Into<IdentificationType>) -> Result<Identification> {
        let id_type = id_type.into().as_raw();
        self.run(move |s| {
            Box::pin(async move {
                let info = s.connected()?;
                let r = s.exchange(Command::GetId, &[id_type]).await?;
                s.require(&r, 8)?;
                let length = info.byte_order.u32(&r[4..8]);
                if r[1] == 1 && length > u32::from(info.max_cto - 8) {
                    s.desynchronized = true;
                    return Err(malformed("GET_ID inline data exceeds MAX_CTO", &r));
                }
                let data = match r[1] {
                    0 => Vec::new(),
                    1 => {
                        s.require(&r, 8 + length as usize)?;
                        r[8..8 + length as usize].to_vec()
                    }
                    _ => return Err(malformed("unsupported GET_ID transfer mode", &r)),
                };
                Ok(Identification {
                    mode: r[1],
                    length,
                    data,
                })
            })
        })
        .await
    }

    /// GET_SEED: return one response segment. Mode 0 starts; mode 1 continues.
    pub async fn get_seed(&self, mode: u8, resource: impl Into<Resource>) -> Result<Seed> {
        let resource = resource.into().as_raw();
        if mode > 1 || (mode == 0 && !matches!(resource, 1 | 4 | 8 | 16 | 32)) {
            return Err(Error::InvalidArgument("invalid GET_SEED mode or resource".into()).into());
        }
        self.run(move |s| {
            Box::pin(async move {
                let capacity = usize::from(s.connected()?.max_cto) - 2;
                let r = s.exchange(Command::GetSeed, &[mode, resource]).await?;
                s.require(&r, 2)?;
                let length = r[1];
                let size = usize::from(length).min(capacity);
                s.require(&r, 2 + size)?;
                Ok(Seed {
                    length,
                    data: r[2..2 + size].to_vec(),
                })
            })
        })
        .await
    }

    /// UNLOCK: send one key segment. Length counts this and all remaining key bytes.
    pub async fn unlock(&self, length: u8, key: &[u8]) -> Result<u8> {
        let key = key.to_vec();
        self.run(move |s| {
            Box::pin(async move {
                let capacity = usize::from(s.connected()?.max_cto) - 2;
                if length == 0 || key.len() != usize::from(length).min(capacity) {
                    return Err(
                        Error::InvalidArgument("invalid UNLOCK segment length".into()).into(),
                    );
                }
                let mut data = vec![length];
                data.extend(key);
                let r = s.exchange(Command::Unlock, &data).await?;
                s.require(&r, 2)?;
                Ok(r[1])
            })
        })
        .await
    }

    pub async fn set_mta(&self, address: u32, address_extension: u8) -> Result<()> {
        self.run(move |s| Box::pin(s.set_mta(address, address_extension)))
            .await
    }

    /// UPLOAD: read one response from MTA. Size is bytes, aligned to AG.
    pub async fn upload(&self, size: usize) -> Result<Vec<u8>> {
        self.run(move |s| Box::pin(s.upload(size))).await
    }

    /// Read one SHORT_UPLOAD response. Size is bytes, and must be a multiple of AG.
    /// The ECU address is measured in elements of its negotiated address granularity.
    pub async fn short_upload(
        &self,
        address: u32,
        size: usize,
        address_extension: u8,
    ) -> Result<Vec<u8>> {
        self.run(move |s| Box::pin(s.short_upload(address, size, address_extension)))
            .await
    }

    /// DOWNLOAD: write one packet at MTA. Data must fit the negotiated MAX_CTO.
    pub async fn download(&self, data: &[u8]) -> Result<()> {
        let data = data.to_vec();
        self.run(move |s| Box::pin(async move { s.download(&data).await }))
            .await
    }

    /// BUILD_CHECKSUM at MTA. Block size is ECU address elements, not bytes.
    pub async fn build_checksum(&self, block_size: u32) -> Result<Checksum> {
        self.run(move |s| {
            Box::pin(async move {
                let order = s.connected()?.byte_order;
                let mut data = vec![0, 0, 0];
                data.extend_from_slice(&order.encode_u32(block_size));
                let r = s.exchange(Command::BuildChecksum, &data).await?;
                s.require(&r, 8)?;
                Ok(Checksum {
                    checksum_type: r[1],
                    checksum: order.u32(&r[4..8]),
                })
            })
        })
        .await
    }
}

fn validate_config(config: XCPConfig) -> Result<()> {
    if config.timeout.is_zero()
        || std::time::Instant::now()
            .checked_add(config.timeout)
            .is_none()
    {
        return Err(Error::InvalidArgument("XCP timeout must be positive".into()).into());
    }
    Ok(())
}

struct Session {
    link: Option<Box<dyn XCPTransport>>,
    config: XCPConfig,
    info: Option<ConnectInfo>,
    desynchronized: bool,
}

impl Session {
    fn connected(&self) -> Result<ConnectInfo> {
        self.info.ok_or_else(|| Error::NotConnected.into())
    }

    fn require(&mut self, response: &[u8], minimum: usize) -> Result<()> {
        if response.len() < minimum {
            self.desynchronized = true;
            return Err(malformed(
                format!("expected at least {minimum} bytes"),
                response,
            ));
        }
        Ok(())
    }

    fn validate_size(&self, size: usize) -> Result<()> {
        if size % usize::from(self.connected()?.address_granularity) != 0 {
            return Err(Error::InvalidArgument(
                "byte count must be a multiple of ECU address granularity".into(),
            )
            .into());
        }
        Ok(())
    }

    async fn connect(&mut self, mode: u8) -> Result<ConnectInfo> {
        if mode > 1 {
            return Err(Error::InvalidArgument("CONNECT mode must be 0 or 1".into()).into());
        }
        let response = self.exchange(Command::Connect, &[mode]).await?;
        self.require(&response, 8)?;
        let order = if response[2] & 1 == 0 {
            ByteOrder::LittleEndian
        } else {
            ByteOrder::BigEndian
        };
        let ag = match (response[2] >> 1) & 3 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => {
                self.desynchronized = true;
                return Err(malformed("reserved address granularity", &response));
            }
        };
        let info = ConnectInfo {
            resource: response[1],
            comm_mode_basic: response[2],
            byte_order: order,
            address_granularity: ag,
            max_cto: response[3],
            max_dto: order.u16(&response[4..6]),
            protocol_version: response[6],
            transport_version: response[7],
        };
        if info.max_cto < 8
            || info.max_dto < 8
            || info.max_cto % ag != 0
            || info.max_dto % u16::from(ag) != 0
            || usize::from(info.max_cto)
                > self
                    .link
                    .as_ref()
                    .ok_or(crate::Error::Disconnected)?
                    .limits()
                    .max_tx_packet_size
            || usize::from(info.max_dto) > self.link.as_ref().unwrap().limits().max_rx_packet_size
        {
            self.desynchronized = true;
            return Err(malformed(
                "invalid CONNECT packet limits for this transport",
                &response,
            ));
        }
        self.info = Some(info);
        Ok(info)
    }

    async fn exchange(&mut self, command: Command, data: &[u8]) -> Result<Vec<u8>> {
        let synchronize = command == Command::Synch;
        if self.desynchronized && !synchronize {
            return Err(Error::SynchronizationRequired.into());
        }
        if command.as_raw() < 0xc0 {
            return Err(
                Error::InvalidArgument("XCP command codes must be at least 0xc0".into()).into(),
            );
        }
        let capacity = if command == Command::Connect {
            8
        } else {
            self.info.map(|i| usize::from(i.max_cto)).unwrap_or(8)
        };
        if command != Command::Connect && !synchronize {
            self.connected()?;
        }
        if data.len() + 1 > capacity {
            return Err(Error::InvalidArgument("command exceeds negotiated MAX_CTO".into()).into());
        }
        if synchronize {
            self.link
                .as_mut()
                .ok_or(crate::Error::Disconnected)?
                .reset()?;
        }
        let mut request = vec![command.as_raw()];
        request.extend_from_slice(data);
        self.desynchronized = true;
        let result = tokio::time::timeout(
            self.config.timeout,
            self.exchange_inner(command, &request, capacity, synchronize),
        )
        .await;
        match result {
            Ok(Ok(response)) => {
                self.desynchronized = false;
                Ok(response)
            }
            Ok(Err(error @ crate::Error::XCPError(Error::NegativeResponse { .. }))) => {
                self.desynchronized = false;
                if matches!(
                    &error,
                    crate::Error::XCPError(Error::NegativeResponse {
                        command: Command::Unlock,
                        code: ErrorCode::AccessLocked,
                        ..
                    })
                ) {
                    self.info = None;
                }
                Err(error)
            }
            Ok(Err(error)) => Err(error),
            Err(_) => Err(crate::Error::Timeout),
        }
    }

    async fn exchange_inner(
        &mut self,
        command: Command,
        request: &[u8],
        capacity: usize,
        synchronize: bool,
    ) -> Result<Vec<u8>> {
        let link = self.link.as_mut().ok_or(crate::Error::Disconnected)?;
        link.send(request).await?;
        loop {
            let mut response = self
                .link
                .as_mut()
                .ok_or(crate::Error::Disconnected)?
                .recv()
                .await?;
            let Some(&pid) = response.first() else {
                return Err(malformed("empty XCP packet", &response));
            };
            if pid >= 0xfe && response.len() > capacity {
                if self.link.as_ref().is_some_and(|link| link.limits().padded) {
                    // CAN tail fill is outside the XCP packet; Ethernet has no such tail.
                    response.truncate(capacity);
                } else {
                    return Err(malformed("control response exceeds MAX_CTO", &response));
                }
            }
            match pid {
                0xff if !synchronize => return Ok(response),
                0xff => {} // Drain a late response until SYNCH's distinctive error arrives.
                0xfe => {
                    if response.len() < 2 {
                        return Err(malformed("missing XCP error code", &response));
                    }
                    if synchronize {
                        if response[1] == 0 {
                            return Ok(response);
                        }
                        continue;
                    }
                    return Err(Error::NegativeResponse {
                        command,
                        code: ErrorCode::from_raw(response[1]),
                        data: response[2..].to_vec(),
                    }
                    .into());
                }
                _ => {
                    if pid == 0xfd && response.get(1) == Some(&7) {
                        self.info = None;
                        return Err(crate::Error::Disconnected);
                    }
                    // Unsolicited packets do not complete a command exchange.
                }
            }
        }
    }

    async fn set_mta(&mut self, address: u32, extension: u8) -> Result<()> {
        let mut data = vec![0, 0, extension];
        data.extend_from_slice(&self.connected()?.byte_order.encode_u32(address));
        self.exchange(Command::SetMta, &data).await.map(|_| ())
    }

    async fn short_upload(&mut self, address: u32, size: usize, extension: u8) -> Result<Vec<u8>> {
        let info = self.connected()?;
        self.validate_size(size)?;
        if size == 0 || size > info.max_upload_size() {
            return Err(Error::InvalidArgument(format!(
                "SHORT_UPLOAD size must be 1..={} bytes, aligned to AG",
                info.max_upload_size()
            ))
            .into());
        }
        let ag = usize::from(info.address_granularity);
        let mut data = vec![(size / ag) as u8, 0, extension];
        data.extend_from_slice(&info.byte_order.encode_u32(address));
        let response = self.exchange(Command::ShortUpload, &data).await?;
        self.require(&response, ag + size)?;
        Ok(response[ag..ag + size].to_vec())
    }

    async fn upload(&mut self, size: usize) -> Result<Vec<u8>> {
        let info = self.connected()?;
        self.validate_size(size)?;
        if size == 0 || size > info.max_upload_size() {
            return Err(Error::InvalidArgument("UPLOAD size exceeds one response".into()).into());
        }
        let ag = usize::from(info.address_granularity);
        let response = self.exchange(Command::Upload, &[(size / ag) as u8]).await?;
        self.require(&response, ag + size)?;
        Ok(response[ag..ag + size].to_vec())
    }

    async fn download(&mut self, data: &[u8]) -> Result<()> {
        let info = self.connected()?;
        self.validate_size(data.len())?;
        let ag = usize::from(info.address_granularity);
        let offset = ag.max(2);
        if data.is_empty() || offset + data.len() > usize::from(info.max_cto) {
            return Err(Error::InvalidArgument("DOWNLOAD data exceeds one packet".into()).into());
        }
        let mut request = vec![0; offset - 1];
        request[0] = (data.len() / ag) as u8;
        request.extend_from_slice(data);
        self.exchange(Command::Download, &request).await.map(|_| ())
    }
}
