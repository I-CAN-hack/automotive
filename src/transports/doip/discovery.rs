use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::net::UdpSocket;

use super::{frame::*, DoIP, DoIPConfig, Error, DOIP_PORT};
use crate::Result;

/// Vehicle identification request variant (ISO 13400-2:2025 §7.4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum VehicleIdentification {
    #[default]
    All,
    Eid([u8; 6]),
    Vin([u8; 17]),
}

/// UDP discovery settings. Defaults send an IPv4 limited broadcast from a
/// dynamic port (49152–65535) and collect responses for two seconds.
///
/// Passive reception uses [`Self::passive`], which binds port 13400 and sends
/// nothing. IPv4 passive listeners normally bind 0.0.0.0 to receive broadcasts
/// from all adapters. IPv6 multicast requires [`Self::ipv6`] with a nonzero
/// OS interface index, available on Windows and Unix. No device names, raw
/// sockets, administrator privileges, or Linux-only socket options are needed.
///
/// To select an IPv4 source address, set `bind_address`. On multihomed hosts,
/// a subnet-directed broadcast `destination` can also select the desired route.
/// IPv6 link-local unicast destinations must carry their scope ID. The two
/// families use separate sockets, so an IPv6 socket never consumes IPv4 packets.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiscoveryConfig {
    pub bind_address: SocketAddr,
    pub destination: SocketAddr,
    /// IPv6 multicast interface index. Required for joining/sending ff02::1.
    pub interface_index: Option<u32>,
    /// Protocol version 0xff identifies vehicles regardless of their version.
    pub protocol_version: u8,
    pub identification: VehicleIdentification,
    /// Collection window; ISO 13400-2 Table 12 specifies at least two seconds.
    pub timeout: Duration,
    /// Opt-in socket sharing. A second listener may receive only some unicast
    /// packets on Windows; leave disabled unless sharing multicast reception.
    pub reuse_address: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            bind_address: (Ipv4Addr::UNSPECIFIED, 0).into(),
            destination: (Ipv4Addr::BROADCAST, DOIP_PORT).into(),
            interface_index: None,
            protocol_version: 0xff,
            identification: VehicleIdentification::All,
            timeout: Duration::from_secs(2),
            reuse_address: false,
        }
    }
}

impl DiscoveryConfig {
    /// Link-local IPv6 multicast discovery on one explicitly selected adapter.
    /// Use the OS IPv6 interface index, not a Linux-specific device name.
    pub fn ipv6(interface_index: u32) -> Self {
        Self {
            bind_address: (Ipv6Addr::UNSPECIFIED, 0).into(),
            destination: SocketAddrV6::new(
                "ff02::1".parse().unwrap(),
                DOIP_PORT,
                0,
                interface_index,
            )
            .into(),
            interface_index: Some(interface_index),
            ..Self::default()
        }
    }

    /// Bind the announcement port instead of an ephemeral request port.
    /// [`Discovery::recv`] then listens without transmitting anything.
    pub fn passive(mut self) -> Self {
        self.bind_address.set_port(DOIP_PORT);
        self
    }
}

/// Parsed vehicle identification response or unsolicited announcement.
/// Raw VIN bytes preserve unconfigured VINs. `address` includes the UDP source
/// port, which need not be 13400; [`Self::connect`] uses TCP port 13400 instead.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VehicleAnnouncement {
    pub address: SocketAddr,
    pub protocol_version: u8,
    pub vin: [u8; 17],
    pub logical_address: u16,
    pub eid: [u8; 6],
    pub gid: [u8; 6],
    pub further_action: u8,
    pub vin_gid_sync_status: Option<u8>,
}

impl VehicleAnnouncement {
    /// Connect to this entity with its advertised version and logical address,
    /// tester address 0x0e00, and default activation. `further_action == 0x10`
    /// requests central-security activation; OEM authentication remains external.
    pub async fn connect(&self) -> Result<DoIP> {
        let mut config = DoIPConfig::new(self.logical_address);
        config.protocol_version = self.protocol_version;
        if self.further_action == 0x10 {
            config.activation_type = 0xe0;
        }
        self.connect_with_config(config).await
    }

    /// Override the target ECU, tester address, activation, and TCP settings.
    /// For a nonstandard TCP port use [`DoIP::connect_with_config`] directly.
    pub async fn connect_with_config(&self, config: DoIPConfig) -> Result<DoIP> {
        let mut address = self.address;
        address.set_port(DOIP_PORT);
        DoIP::connect_with_config(address, config).await
    }
}

/// A bound discovery socket. `bind` starts listening before any request is sent.
/// Use separate sockets for active requests and passive announcements, because
/// requests originate from dynamic ports while announcements arrive on 13400.
pub struct Discovery {
    socket: UdpSocket,
    config: DiscoveryConfig,
}

impl Discovery {
    /// Listen for unsolicited IPv4 announcements on port 13400, sending nothing.
    pub fn listen() -> Result<Self> {
        Self::bind(DiscoveryConfig::default().passive())
    }

    pub fn bind(mut config: DiscoveryConfig) -> Result<Self> {
        if config.bind_address.is_ipv4() != config.destination.is_ipv4() {
            return Err(
                Error::InvalidConfig("discovery address families must match".into()).into(),
            );
        }
        if config.timeout.is_zero() || !matches!(config.protocol_version, 2..=4 | 0xff) {
            return Err(Error::InvalidConfig(
                "discovery timeout must be positive and version must be 2, 3, 4, or 255".into(),
            )
            .into());
        }
        let domain = if config.bind_address.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(config.reuse_address)?;
        if config.bind_address.is_ipv6() {
            socket.set_only_v6(true)?;
        }
        if config.bind_address.is_ipv4() {
            socket.set_broadcast(true)?;
        }
        socket.set_nonblocking(true)?;
        if config.bind_address.port() == 0 {
            // ISO 13400-2 §11.2 requires the IANA dynamic range. Some OSes
            // assign lower ephemeral ports when binding port zero.
            let start = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
                % 16384;
            let mut bound = false;
            for offset in 0..16384 {
                let mut address = config.bind_address;
                address.set_port((49152 + (start + offset) % 16384) as u16);
                match socket.bind(&address.into()) {
                    Ok(()) => {
                        bound = true;
                        break;
                    }
                    // Windows also reports WSAEACCES for excluded ports or
                    // sockets held with exclusive access. These candidates
                    // are unavailable, but another dynamic port may work.
                    Err(error)
                        if error.kind() == std::io::ErrorKind::AddrInUse
                            || (cfg!(windows) && error.raw_os_error() == Some(10013)) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            if !bound {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    "no free DoIP dynamic UDP port",
                )
                .into());
            }
        } else {
            socket.bind(&config.bind_address.into())?;
        }
        if let SocketAddr::V6(ref mut destination) = config.destination {
            if destination.ip().is_multicast() {
                let index = config
                    .interface_index
                    .filter(|index| *index != 0)
                    .ok_or_else(|| {
                        Error::InvalidConfig(
                            "IPv6 multicast requires a nonzero interface_index".into(),
                        )
                    })?;
                if destination.scope_id() != 0 && destination.scope_id() != index {
                    return Err(Error::InvalidConfig(
                        "IPv6 scope ID and interface_index differ".into(),
                    )
                    .into());
                }
                socket.set_multicast_if_v6(index)?;
                socket.join_multicast_v6(destination.ip(), index)?;
                destination.set_scope_id(index);
            }
        }
        Ok(Self {
            socket: UdpSocket::from_std(socket.into())?,
            config,
        })
    }

    pub fn local_address(&self) -> Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }

    /// Send an identification request to the configured broadcast, multicast,
    /// or unicast destination. Responses arrive on this socket's local port.
    pub async fn request(&self) -> Result<()> {
        let (kind, payload): (u16, &[u8]) = match &self.config.identification {
            VehicleIdentification::All => (1, &[]),
            VehicleIdentification::Eid(eid) => (2, eid),
            VehicleIdentification::Vin(vin) => (3, vin),
        };
        self.socket
            .send_to(
                &encode(self.config.protocol_version, kind, payload),
                self.config.destination,
            )
            .await?;
        Ok(())
    }

    /// Wait indefinitely for one announcement/response without sending a request.
    /// Cancellation is safe; malformed and unrelated UDP datagrams are discarded.
    /// IPv6 scope IDs are preserved for subsequent link-local TCP connections.
    pub async fn recv(&self) -> Result<VehicleAnnouncement> {
        let mut bytes = [0u8; 65536];
        loop {
            let (size, mut address) = self.socket.recv_from(&mut bytes).await?;
            if address.ip().is_multicast()
                || address.ip().is_unspecified()
                || address.ip() == Ipv4Addr::BROADCAST
            {
                continue;
            }
            let Ok((version, kind, length)) = header(&bytes[..size], bytes.len() - 8) else {
                continue;
            };
            if kind != ANNOUNCEMENT || size != length + 8 {
                continue;
            }
            if let SocketAddr::V6(ref mut source) = address {
                if source.ip().is_unicast_link_local() && source.scope_id() == 0 {
                    if let Some(index) = self.config.interface_index {
                        source.set_scope_id(index);
                    }
                }
            }
            let payload = &bytes[8..size];
            let announcement = VehicleAnnouncement {
                address,
                protocol_version: version,
                vin: payload[..17].try_into().unwrap(),
                logical_address: u16::from_be_bytes([payload[17], payload[18]]),
                eid: payload[19..25].try_into().unwrap(),
                gid: payload[25..31].try_into().unwrap(),
                further_action: payload[31],
                vin_gid_sync_status: payload.get(32).copied(),
            };
            let matches = match &self.config.identification {
                VehicleIdentification::All => true,
                VehicleIdentification::Eid(eid) => *eid == announcement.eid,
                VehicleIdentification::Vin(vin) => *vin == announcement.vin,
            };
            if matches {
                return Ok(announcement);
            }
        }
    }

    /// Trigger identification and collect unique entity responses for `timeout`.
    /// The deadline is fixed even if malformed or repeated datagrams arrive.
    pub async fn discover(&self) -> Result<Vec<VehicleAnnouncement>> {
        self.request().await?;
        let deadline = tokio::time::Instant::now() + self.config.timeout;
        let mut vehicles: Vec<VehicleAnnouncement> = Vec::new();
        while let Ok(result) = tokio::time::timeout_at(deadline, self.recv()).await {
            let vehicle = result?;
            if let Some(previous) = vehicles.iter_mut().find(|previous| {
                previous.address.ip() == vehicle.address.ip() && previous.eid == vehicle.eid
            }) {
                *previous = vehicle;
            } else {
                vehicles.push(vehicle);
            }
        }
        Ok(vehicles)
    }
}

/// Identify vehicles using IPv4 limited broadcast, with a two-second collection window.
pub async fn discover() -> Result<Vec<VehicleAnnouncement>> {
    discover_with_config(DiscoveryConfig::default()).await
}

/// Identify vehicles with optional source binding, IPv6, VIN/EID filters, and timing.
pub async fn discover_with_config(config: DiscoveryConfig) -> Result<Vec<VehicleAnnouncement>> {
    Discovery::bind(config)?.discover().await
}
