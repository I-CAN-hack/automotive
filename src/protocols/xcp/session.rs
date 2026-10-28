//! Serialized XCP request/response exchanges and session negotiation.

use super::{
    error::malformed, ByteOrder, Command, ConnectInfo, Error, ErrorCode, XCPConfig, XCPTransport,
};
use crate::Result;

pub(super) struct Session {
    pub(super) link: Option<Box<dyn XCPTransport>>,
    pub(super) config: XCPConfig,
    pub(super) info: Option<ConnectInfo>,
    pub(super) desynchronized: bool,
}

impl Session {
    pub(super) fn connected(&self) -> Result<ConnectInfo> {
        self.info.ok_or_else(|| Error::NotConnected.into())
    }

    pub(super) fn require(&mut self, response: &[u8], minimum: usize) -> Result<()> {
        if response.len() < minimum {
            self.desynchronized = true;
            return Err(malformed(
                format!("expected at least {minimum} bytes"),
                response,
            ));
        }
        Ok(())
    }

    pub(super) fn elements(&self, size: usize, overhead: usize) -> Result<u8> {
        let info = self.connected()?;
        let ag = usize::from(info.address_granularity);
        if size == 0 || size % ag != 0 || size > usize::from(info.max_cto).saturating_sub(overhead)
        {
            return Err(Error::InvalidArgument(
                "data size must fit one packet and align to AG".into(),
            )
            .into());
        }
        Ok((size / ag) as u8)
    }

    pub(super) async fn connect(&mut self, mode: u8) -> Result<ConnectInfo> {
        if mode > 1 {
            return Err(Error::InvalidArgument("CONNECT mode must be 0 or 1".into()).into());
        }
        let response = self.exchange(Command::Connect, &[mode], 8).await?;
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
        let limits = self
            .link
            .as_ref()
            .ok_or(crate::Error::Disconnected)?
            .limits();
        if info.max_cto < 8
            || info.max_dto < 8
            || info.max_cto % ag != 0
            || info.max_dto % u16::from(ag) != 0
            || usize::from(info.max_cto) > limits.max_tx_packet_size
            || usize::from(info.max_dto) > limits.max_rx_packet_size
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

    pub(super) async fn exchange(
        &mut self,
        command: Command,
        data: &[u8],
        minimum: usize,
    ) -> Result<Vec<u8>> {
        let synchronize = command == Command::Synch;
        if self.desynchronized && !synchronize {
            return Err(Error::SynchronizationRequired.into());
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
        .await
        .map_err(|_| crate::Error::Timeout)?;
        match &result {
            Ok(response) => {
                self.require(response, minimum)?;
                self.desynchronized = false;
            }
            Err(crate::Error::XCPError(Error::NegativeResponse { code, .. })) => {
                self.desynchronized = false;
                if command == Command::Unlock && *code == ErrorCode::AccessLocked {
                    self.info = None;
                }
            }
            _ => {}
        }
        result
    }

    async fn exchange_inner(
        &mut self,
        command: Command,
        request: &[u8],
        capacity: usize,
        synchronize: bool,
    ) -> Result<Vec<u8>> {
        let link = self.link.as_mut().ok_or(crate::Error::Disconnected)?;
        let padded = link.limits().padded;
        link.send(request).await?;
        loop {
            let mut response = link.recv().await?;
            let Some(&pid) = response.first() else {
                return Err(malformed("empty XCP packet", &response));
            };
            if pid >= 0xfe && response.len() > capacity {
                if padded {
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
}
