//! XCP command client for Classical CAN, CAN FD, TCP/IP, and UDP/IP.
//!
//! Wraps individual ASAM XCP 1.5 commands (sections 7.5.1 and 7.5.2),
//! CAN transport framing, and Ethernet LEN/CTR framing. Each command call sends
//! one request. Callers handle transfer loops using the CONNECT capabilities.
//!
//! Commands are serialized with a mutex. A cancelled exchange, timeout, or
//! malformed response requires `synchronize()` before the next command. An
//! interrupted TCP write requires a new client. Commands are never retried.
//! Optional commands remain subject to ECU support and resource protection.

mod constants;
mod error;
mod session;
pub mod transport;
mod types;

pub use constants::*;
pub use error::{Error, ErrorCode};
pub use transport::{PacketLimits, XCPCanConfig, XCPOnCAN, XCPOnTCP, XCPOnUDP, XCPTransport};
pub use types::*;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::{Mutex, MutexGuard};

use crate::Result;
use error::malformed;
use session::Session;

/// An XCP client retaining its CAN adapter or Ethernet socket.
#[derive(Clone)]
pub struct XCPClient {
    inner: Arc<Mutex<Session>>,
    closed: Arc<AtomicBool>,
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
            closed: Arc::new(AtomicBool::new(false)),
        })
    }

    async fn session(&self) -> Result<MutexGuard<'_, Session>> {
        if self.is_closed() {
            return Err(crate::Error::Disconnected);
        }
        let session = self.inner.lock().await;
        if self.is_closed() {
            return Err(crate::Error::Disconnected);
        }
        Ok(session)
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Stop accepting commands and release the transport after the active exchange.
    /// Does not transmit DISCONNECT; call `disconnect` first for a graceful exit.
    pub async fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        let mut session = self.inner.lock().await;
        session.link.take();
        session.info = None;
    }

    pub async fn connection_info(&self) -> Option<ConnectInfo> {
        self.inner.lock().await.info
    }

    pub async fn connect(&self, mode: impl Into<ConnectMode>) -> Result<ConnectInfo> {
        let mode = mode.into().as_raw();
        self.session().await?.connect(mode).await
    }

    pub async fn disconnect(&self) -> Result<()> {
        let mut session = self.session().await?;
        session.exchange(Command::Disconnect, &[], 1).await?;
        session.info = None;
        Ok(())
    }

    /// Recover command synchronization. SYNCH succeeds with ERR_CMD_SYNCH (0x00).
    pub async fn synchronize(&self) -> Result<()> {
        self.session()
            .await?
            .exchange(Command::Synch, &[], 2)
            .await
            .map(|_| ())
    }

    pub async fn get_status(&self) -> Result<Status> {
        let mut session = self.session().await?;
        let order = session.connected()?.byte_order;
        let response = session.exchange(Command::GetStatus, &[], 6).await?;
        Ok(Status {
            session_status: response[1],
            resource_protection_status: response[2],
            session_configuration_id: order.u16(&response[4..6]),
        })
    }

    pub async fn get_comm_mode_info(&self) -> Result<CommunicationModeInfo> {
        let mut session = self.session().await?;
        let r = session.exchange(Command::GetCommModeInfo, &[], 8).await?;
        Ok(CommunicationModeInfo {
            comm_mode_optional: r[2],
            max_bs: r[4],
            min_st: r[5],
            queue_size: r[6],
            driver_version: r[7],
        })
    }

    pub async fn get_id(&self, id_type: impl Into<IdentificationType>) -> Result<Identification> {
        let id_type = id_type.into().as_raw();
        let mut session = self.session().await?;
        let info = session.connected()?;
        let r = session.exchange(Command::GetId, &[id_type], 8).await?;
        let length = info.byte_order.u32(&r[4..8]);
        if r[1] == 1 && length > u32::from(info.max_cto - 8) {
            session.desynchronized = true;
            return Err(malformed("GET_ID inline data exceeds MAX_CTO", &r));
        }
        let data = match r[1] {
            0 => Vec::new(),
            1 => {
                session.require(&r, 8 + length as usize)?;
                r[8..8 + length as usize].to_vec()
            }
            _ => return Err(malformed("unsupported GET_ID transfer mode", &r)),
        };
        Ok(Identification {
            mode: r[1],
            length,
            data,
        })
    }

    /// GET_SEED: return one response segment. Mode 0 starts; mode 1 continues.
    pub async fn get_seed(&self, mode: u8, resource: impl Into<Resource>) -> Result<Seed> {
        let resource = resource.into().as_raw();
        if mode > 1 || (mode == 0 && !matches!(resource, 1 | 4 | 8 | 16 | 32)) {
            return Err(Error::InvalidArgument("invalid GET_SEED mode or resource".into()).into());
        }
        let mut session = self.session().await?;
        let capacity = usize::from(session.connected()?.max_cto) - 2;
        let r = session
            .exchange(Command::GetSeed, &[mode, resource], 2)
            .await?;
        let length = r[1];
        let size = usize::from(length).min(capacity);
        session.require(&r, 2 + size)?;
        Ok(Seed {
            length,
            data: r[2..2 + size].to_vec(),
        })
    }

    /// UNLOCK: send one key segment. Length counts this and all remaining key bytes.
    pub async fn unlock(&self, length: u8, key: &[u8]) -> Result<u8> {
        let mut session = self.session().await?;
        let capacity = usize::from(session.connected()?.max_cto) - 2;
        if length == 0 || key.len() != usize::from(length).min(capacity) {
            return Err(Error::InvalidArgument("invalid UNLOCK segment length".into()).into());
        }
        let mut data = vec![length];
        data.extend_from_slice(key);
        let r = session.exchange(Command::Unlock, &data, 2).await?;
        Ok(r[1])
    }

    pub async fn set_mta(&self, address: u32, address_extension: u8) -> Result<()> {
        let mut session = self.session().await?;
        let mut data = vec![0, 0, address_extension];
        data.extend_from_slice(&session.connected()?.byte_order.encode_u32(address));
        session
            .exchange(Command::SetMta, &data, 1)
            .await
            .map(|_| ())
    }

    /// UPLOAD: read one response from MTA. Size is bytes, aligned to AG.
    pub async fn upload(&self, size: usize) -> Result<Vec<u8>> {
        let mut session = self.session().await?;
        let ag = usize::from(session.connected()?.address_granularity);
        let count = session.elements(size, ag)?;
        let response = session
            .exchange(Command::Upload, &[count], ag + size)
            .await?;
        Ok(response[ag..ag + size].to_vec())
    }

    /// SHORT_UPLOAD: read one response at an address measured in ECU elements.
    pub async fn short_upload(
        &self,
        address: u32,
        size: usize,
        address_extension: u8,
    ) -> Result<Vec<u8>> {
        let mut session = self.session().await?;
        let info = session.connected()?;
        let ag = usize::from(info.address_granularity);
        let count = session.elements(size, ag)?;
        let mut data = vec![count, 0, address_extension];
        data.extend_from_slice(&info.byte_order.encode_u32(address));
        let response = session
            .exchange(Command::ShortUpload, &data, ag + size)
            .await?;
        Ok(response[ag..ag + size].to_vec())
    }

    /// DOWNLOAD: write one packet at MTA. Data must fit the negotiated MAX_CTO.
    pub async fn download(&self, data: &[u8]) -> Result<()> {
        let mut session = self.session().await?;
        let offset = usize::from(session.connected()?.address_granularity).max(2);
        let count = session.elements(data.len(), offset)?;
        let mut request = vec![0; offset - 1];
        request[0] = count;
        request.extend_from_slice(data);
        session
            .exchange(Command::Download, &request, 1)
            .await
            .map(|_| ())
    }

    /// BUILD_CHECKSUM at MTA. Block size is ECU address elements, not bytes.
    pub async fn build_checksum(&self, block_size: u32) -> Result<Checksum> {
        let mut session = self.session().await?;
        let order = session.connected()?.byte_order;
        let mut data = vec![0, 0, 0];
        data.extend_from_slice(&order.encode_u32(block_size));
        let r = session.exchange(Command::BuildChecksum, &data, 8).await?;
        Ok(Checksum {
            checksum_type: r[1],
            checksum: order.u32(&r[4..8]),
        })
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
