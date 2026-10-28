//! CCP 2.1 client for reading and writing ECU memory on Classical CAN.
//!
//! Implements session setup, seed/key access and memory transfer primitives
//! from CCP 2.1 sections 12.1–12.9, 12.15 and 12.28. Each method sends one
//! command; callers implement transfer loops. Sizes are bytes. Address byte
//! order must match the ECU description; station addresses are always LE.
//!
//! Clones share a serialized session. Commands are never retried automatically.
//! After cancellation, timeout or a malformed response, CONNECT with a new
//! counter restores command matching. Re-establish MTA and access privileges
//! before continuing: the interrupted command may have executed on the ECU.

mod constants;
mod error;
pub mod transport;
mod types;

pub use constants::*;
pub use error::{Error, ErrorCode};
pub use transport::{CCPCanConfig, CCPOnCAN, CCPTransport};
pub use types::*;

use crate::Result;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::{Mutex, MutexGuard};

/// A CCP command client retaining its CAN transport.
#[derive(Clone)]
pub struct CCPClient {
    inner: Arc<Mutex<Session>>,
    closed: Arc<AtomicBool>,
}

impl CCPClient {
    pub fn new(transport: impl CCPTransport + 'static) -> Result<Self> {
        Self::with_config(transport, CCPConfig::default())
    }

    pub fn with_config(transport: impl CCPTransport + 'static, config: CCPConfig) -> Result<Self> {
        if config.timeout.is_zero()
            || std::time::Instant::now()
                .checked_add(config.timeout)
                .is_none()
        {
            return Err(Error::InvalidArgument(
                "timeout must be positive and representable".into(),
            )
            .into());
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(Session {
                link: Some(Box::new(transport)),
                config,
                station: None,
                counter: 0,
                uncertain: false,
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

    /// Release the transport after the active exchange; does not send DISCONNECT.
    pub async fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        let mut session = self.inner.lock().await;
        session.link.take();
        session.station = None;
    }

    /// Connect directly to a known station. Does not negotiate ECU byte order.
    pub async fn connect(&self, station_address: u16) -> Result<()> {
        let mut s = self.session().await?;
        s.station = None;
        s.exchange(Command::Connect, &station_address.to_le_bytes())
            .await?;
        s.station = Some(station_address);
        Ok(())
    }

    /// Disconnect temporarily or terminate the calibration session.
    pub async fn disconnect(&self, temporary: bool) -> Result<()> {
        let mut s = self.session().await?;
        let station = s.station.ok_or(Error::NotConnected)?.to_le_bytes();
        s.exchange(
            Command::Disconnect,
            &[u8::from(!temporary), 0, station[0], station[1]],
        )
        .await?;
        s.station = None;
        Ok(())
    }

    pub async fn get_ccp_version(&self, major: u8, minor: u8) -> Result<ProtocolVersion> {
        let r = self
            .session()
            .await?
            .exchange(Command::GetCcpVersion, &[major, minor])
            .await?;
        Ok(ProtocolVersion {
            major: r[3],
            minor: r[4],
        })
    }

    /// Exchange station IDs and set MTA0 to the ECU identification bytes.
    pub async fn exchange_id(&self, master_id: &[u8]) -> Result<Identification> {
        let r = self
            .session()
            .await?
            .exchange(Command::ExchangeId, master_id)
            .await?;
        Ok(Identification {
            length: r[3],
            data_type: r[4],
            resource_availability: r[5],
            resource_protection: r[6],
        })
    }

    pub async fn get_seed(&self, resource: impl Into<Resource>) -> Result<Seed> {
        let resource = resource.into().as_raw();
        if !matches!(resource, 1 | 2 | 0x40) {
            return Err(Error::InvalidArgument("GET_SEED requires one CCP resource".into()).into());
        }
        let r = self
            .session()
            .await?
            .exchange(Command::GetSeed, &[resource])
            .await?;
        Ok(Seed {
            protected: r[3] != 0,
            data: [r[4], r[5], r[6], r[7]],
        })
    }

    /// Send up to six key bytes and return the current privilege resource mask.
    pub async fn unlock(&self, key: &[u8]) -> Result<u8> {
        if key.is_empty() {
            return Err(Error::InvalidArgument("key must contain 1–6 bytes".into()).into());
        }
        Ok(self.session().await?.exchange(Command::Unlock, key).await?[3])
    }

    /// Set MTA0 for UPLOAD/DNLOAD, or MTA1 for ECU memory moves.
    pub async fn set_mta(&self, address: u32, address_extension: u8, mta: u8) -> Result<()> {
        if mta > 1 {
            return Err(Error::InvalidArgument("MTA number must be 0 or 1".into()).into());
        }
        let mut s = self.session().await?;
        let mut data = vec![mta, address_extension];
        data.extend(s.config.byte_order.encode_u32(address));
        s.exchange(Command::SetMta, &data).await.map(|_| ())
    }

    /// Read 1–5 bytes from MTA0 and post-increment MTA0 by that byte count.
    pub async fn upload(&self, size: usize) -> Result<Vec<u8>> {
        transfer_size(size)?;
        let r = self
            .session()
            .await?
            .exchange(Command::Upload, &[size as u8])
            .await?;
        Ok(r[3..3 + size].to_vec())
    }

    /// SHORT_UP reads 1–5 bytes at an address without changing MTA0.
    pub async fn short_upload(
        &self,
        address: u32,
        size: usize,
        address_extension: u8,
    ) -> Result<Vec<u8>> {
        transfer_size(size)?;
        let mut s = self.session().await?;
        let mut data = vec![size as u8, address_extension];
        data.extend(s.config.byte_order.encode_u32(address));
        let r = s.exchange(Command::ShortUpload, &data).await?;
        Ok(r[3..3 + size].to_vec())
    }

    /// DNLOAD writes 1–5 bytes at MTA0, returning its post-incremented address.
    pub async fn download(&self, data: &[u8]) -> Result<MemoryAddress> {
        transfer_size(data.len())?;
        let mut request = vec![data.len() as u8];
        request.extend(data);
        self.write(Command::Download, &request).await
    }

    /// Optional DNLOAD_6 writes exactly six bytes at MTA0.
    pub async fn download6(&self, data: &[u8; 6]) -> Result<MemoryAddress> {
        self.write(Command::Download6, data).await
    }

    async fn write(&self, command: Command, data: &[u8]) -> Result<MemoryAddress> {
        let mut s = self.session().await?;
        let r = s.exchange(command, data).await?;
        Ok(MemoryAddress {
            address: s.config.byte_order.u32(&r[4..8]),
            address_extension: r[3],
        })
    }
}

fn transfer_size(size: usize) -> Result<()> {
    if !(1..=5).contains(&size) {
        return Err(Error::InvalidArgument("transfer size must be 1–5 bytes".into()).into());
    }
    Ok(())
}

struct Session {
    link: Option<Box<dyn CCPTransport>>,
    config: CCPConfig,
    station: Option<u16>,
    counter: u8,
    uncertain: bool,
}

impl Session {
    async fn exchange(&mut self, command: Command, data: &[u8]) -> Result<[u8; 8]> {
        if data.len() > 6 {
            return Err(
                Error::InvalidArgument("command parameters exceed six bytes".into()).into(),
            );
        }
        if command != Command::Connect {
            if self.uncertain {
                return Err(Error::ReconnectRequired.into());
            }
            self.station.ok_or(Error::NotConnected)?;
        }
        let counter = self.counter;
        self.counter = self.counter.wrapping_add(1);
        let mut request = [0; 8];
        request[0] = command.as_raw();
        request[1] = counter;
        request[2..2 + data.len()].copy_from_slice(data);
        self.uncertain = true; // Also retained if this future is cancelled during I/O.
        let result = tokio::time::timeout(
            self.config.timeout,
            self.exchange_inner(command, counter, &request),
        )
        .await
        .map_err(|_| crate::Error::Timeout)?;
        if result.is_ok()
            || matches!(
                &result,
                Err(crate::Error::CCPError(Error::NegativeResponse { .. }))
            )
        {
            self.uncertain = false;
        }
        result
    }

    async fn exchange_inner(
        &mut self,
        command: Command,
        counter: u8,
        request: &[u8; 8],
    ) -> Result<[u8; 8]> {
        let link = self.link.as_mut().ok_or(crate::Error::Disconnected)?;
        link.send(request).await?;
        loop {
            let r = link.recv().await?;
            let Some(&pid) = r.first() else {
                return Err(Error::MalformedResponse { response: r }.into());
            };
            if pid < 0xfe {
                continue;
            } // DAQ traffic does not acknowledge a command.
            if r.len() != 8 {
                return Err(Error::MalformedResponse { response: r }.into());
            }
            if pid == 0xff && r[2] != counter {
                continue;
            } // Late or unrelated CRM.
            let code = ErrorCode::from_raw(r[1]);
            if matches!(r[1], 0x10..=0x12) {
                continue;
            } // C1: wait for ACK within the original deadline.
            if pid == 0xfe {
                if r[1] <= 1 {
                    continue;
                } // Unsolicited DAQ warning; CTR is unspecified.
                return Err(Error::Event {
                    code,
                    data: r[3..].to_vec(),
                }
                .into());
            }
            if r[1] != 0 {
                if matches!(r[1], 0x20..=0x23) {
                    self.station = None;
                }
                return Err(Error::NegativeResponse {
                    command,
                    code,
                    data: r[3..].to_vec(),
                }
                .into());
            }
            return Ok(r.try_into().unwrap());
        }
    }
}
