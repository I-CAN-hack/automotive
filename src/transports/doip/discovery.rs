//! UDP identification and announcements (ISO 13400-2:2025 §7.4 and §11.2).

use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::net::UdpSocket;
use tokio::time::Instant;

use super::{
    frame::{invalid, Message, Payload, Request},
    DoIP, DoIPConfig, Error, DOIP_PORT,
};
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
/// Incomplete VIN/GID synchronization extends discovery by a five-second
/// per-vehicle wait followed by another request and response collection window.
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
    /// Local UDP endpoint. Port zero selects a dynamic source port for requests.
    pub bind_address: SocketAddr,
    /// Vehicle-side UDP endpoint; defaults to IPv4 broadcast on port 13400.
    pub destination: SocketAddr,
    /// IPv6 multicast interface index. Required for joining/sending ff02::1.
    pub interface_index: Option<u32>,
    /// Protocol version 0xff identifies vehicles regardless of their version.
    pub protocol_version: u8,
    pub identification: VehicleIdentification,
    /// Response collection window after each request; defaults to two seconds.
    /// Incomplete VIN/GID synchronization adds a five-second per-vehicle wait
    /// and another request (ISO 13400-2:2025 Figure 10 and Table 12).
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
    /// Vehicle identification number; raw bytes preserve unconfigured values.
    pub vin: [u8; 17],
    /// Diagnostic address of this entity, independently of its IP address.
    pub logical_address: u16,
    /// Entity identifier, distinguishing DoIP entities before a VIN is configured.
    pub eid: [u8; 6],
    /// Group identifier, associating entities in the same vehicle without a VIN.
    pub gid: [u8; 6],
    /// Additional action code; 0x10 requests routing activation for central security.
    pub further_action: u8,
    /// Optional status: 0x00 is synchronized; 0x10 means VIN/GID data is incomplete.
    pub vin_gid_sync_status: Option<u8>,
}

impl VehicleAnnouncement {
    /// Connect to this entity with its advertised version and logical address,
    /// tester address 0x0e00, and default activation. `further_action == 0x10`
    /// requests central-security activation; OEM authentication remains external.
    pub async fn connect(&self) -> Result<DoIP> {
        let mut config = DoIPConfig::new(self.logical_address);
        config.protocol_version = self.protocol_version;
        // §7.4, 8.DoIP-144 and Tables 6, 54: central security uses type 0xe0.
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
        // §11.2, 4.DoIP-136: listen before sending the first identification request.
        let socket = bind_socket(&mut config)?;
        Ok(Self { socket, config })
    }

    pub fn local_address(&self) -> Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }

    /// Send an identification request to the configured broadcast, multicast,
    /// or unicast destination. Responses arrive on this socket's local port.
    pub async fn request(&self) -> Result<()> {
        let bytes = self
            .config
            .identification
            .request()
            .encode(self.config.protocol_version);
        self.socket.send_to(&bytes, self.config.destination).await?;
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
            let Ok(message) = Message::from_datagram(&bytes[..size], bytes.len() - 8) else {
                continue;
            };
            // Some OSes omit the incoming link-local scope. Retain the selected
            // interface so VehicleAnnouncement::connect can reach the same peer.
            if let SocketAddr::V6(ref mut source) = address {
                if source.ip().is_unicast_link_local() && source.scope_id() == 0 {
                    if let Some(index) = self.config.interface_index {
                        source.set_scope_id(index);
                    }
                }
            }
            let Ok(announcement) = VehicleAnnouncement::from_message(address, message) else {
                continue;
            };
            if self.config.identification.matches(&announcement) {
                return Ok(announcement);
            }
        }
    }

    /// Trigger identification and collect unique entity responses.
    /// An incomplete VIN/GID synchronization status with a valid VIN or GID starts
    /// a five-second timer for that vehicle. On expiry, request identification
    /// again and collect responses for another `timeout` window. Each vehicle's
    /// timer starts only once; duplicate responses do not extend discovery.
    pub async fn discover(&self) -> Result<Vec<VehicleAnnouncement>> {
        self.request().await?;
        let mut state = DiscoveryState::new(self.config.timeout);
        loop {
            // A synchronizing vehicle may reveal more entities after the normal
            // response window ends. Its vehicle timer keeps discovery open.
            let synchronization_deadline = state.synchronization_deadline();
            let deadline = synchronization_deadline.unwrap_or(state.response_deadline);
            let now = Instant::now();
            if now >= deadline {
                if synchronization_deadline.is_none() {
                    return Ok(state.vehicles);
                }
                // §7.4, Figure 10: after the vehicle's synchronization wait,
                // request identification again and allow fresh responses.
                state.expire_synchronization(now);
                self.request().await?;
                state.response_deadline = state
                    .response_deadline
                    .max(Instant::now() + self.config.timeout);
                continue;
            }
            match tokio::time::timeout_at(deadline, self.recv()).await {
                Ok(result) => state.record(result?),
                Err(_) => continue,
            }
        }
    }
}

/// Identify vehicles using IPv4 limited broadcast, with two-second response windows.
/// Incomplete VIN/GID synchronization adds a five-second wait and another request.
pub async fn discover() -> Result<Vec<VehicleAnnouncement>> {
    discover_with_config(DiscoveryConfig::default()).await
}

/// Identify vehicles with optional source binding, IPv6, VIN/EID filters, and timing.
pub async fn discover_with_config(config: DiscoveryConfig) -> Result<Vec<VehicleAnnouncement>> {
    Discovery::bind(config)?.discover().await
}

impl VehicleIdentification {
    fn request(&self) -> Request<'_> {
        match self {
            Self::All => Request::VehicleIdentification,
            Self::Eid(eid) => Request::VehicleIdentificationEid(*eid),
            Self::Vin(vin) => Request::VehicleIdentificationVin(*vin),
        }
    }

    fn matches(&self, announcement: &VehicleAnnouncement) -> bool {
        match self {
            Self::All => true,
            Self::Eid(eid) => *eid == announcement.eid,
            Self::Vin(vin) => *vin == announcement.vin,
        }
    }
}

impl VehicleAnnouncement {
    fn from_message(address: SocketAddr, message: Message) -> Result<Self> {
        let Payload::Announcement(announcement) = message.payload else {
            return Err(invalid("expected vehicle announcement"));
        };
        Ok(Self {
            address,
            protocol_version: message.version,
            vin: announcement.vin,
            logical_address: announcement.logical_address,
            eid: announcement.eid,
            gid: announcement.gid,
            further_action: announcement.further_action,
            vin_gid_sync_status: announcement.vin_gid_sync_status,
        })
    }
}

// VIN/GID synchronization timers belong to vehicles, which can have several entities.
struct DiscoveryState {
    response_deadline: Instant,
    synchronization: Vec<VehicleDiscoveryTimer>,
    vehicles: Vec<VehicleAnnouncement>,
}

struct VehicleDiscoveryTimer {
    vin: Option<[u8; 17]>,
    gid: Option<[u8; 6]>,
    // None means this vehicle's timer already expired. Its recorded identifiers
    // prevent repeated incomplete announcements from starting another wait.
    deadline: Option<Instant>,
}

impl DiscoveryState {
    fn new(response_timeout: Duration) -> Self {
        Self {
            response_deadline: Instant::now() + response_timeout,
            synchronization: Vec::new(),
            vehicles: Vec::new(),
        }
    }

    fn synchronization_deadline(&self) -> Option<Instant> {
        self.synchronization
            .iter()
            .filter_map(|vehicle| vehicle.deadline)
            .min()
    }

    fn expire_synchronization(&mut self, now: Instant) {
        for vehicle in &mut self.synchronization {
            if vehicle.deadline.is_some_and(|deadline| deadline <= now) {
                vehicle.deadline = None;
            }
        }
    }

    fn record(&mut self, vehicle: VehicleAnnouncement) {
        // §7.7, Table 12 (A_Vehicle_Discovery_Timer) and §7.4, Table 7:
        // only incomplete status 0x10 with a valid VIN or GID starts a timer.
        // The timer belongs to the vehicle, which may have several DoIP entities.
        let vin = valid_identifier(&vehicle.vin).then_some(vehicle.vin);
        let gid = valid_identifier(&vehicle.gid).then_some(vehicle.gid);
        if let Some(known) = self.synchronization.iter_mut().find(|known| {
            (vin.is_some() && known.vin == vin) || (gid.is_some() && known.gid == gid)
        }) {
            known.vin = vin.or(known.vin);
            known.gid = gid.or(known.gid);
        } else if vehicle.vin_gid_sync_status == Some(0x10) && (vin.is_some() || gid.is_some()) {
            self.synchronization.push(VehicleDiscoveryTimer {
                vin,
                gid,
                deadline: Some(Instant::now() + Duration::from_secs(5)),
            });
        }

        // §7.4, 8.DoIP-050: announcements are repeated to compensate for UDP
        // loss. Later announcements replace earlier fields for the same peer/EID.
        if let Some(previous) = self.vehicles.iter_mut().find(|previous| {
            previous.address.ip() == vehicle.address.ip() && previous.eid == vehicle.eid
        }) {
            *previous = vehicle;
        } else {
            self.vehicles.push(vehicle);
        }
    }
}

fn valid_identifier(bytes: &[u8]) -> bool {
    // §7.3, Table 1: all-zero and all-0xff identifiers are not configured.
    !bytes.iter().all(|byte| *byte == 0) && !bytes.iter().all(|byte| *byte == 0xff)
}

fn bind_socket(config: &mut DiscoveryConfig) -> Result<UdpSocket> {
    if config.bind_address.is_ipv4() != config.destination.is_ipv4() {
        return Err(Error::InvalidConfig("discovery address families must match".into()).into());
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
    } else {
        socket.set_broadcast(true)?;
    }
    socket.set_nonblocking(true)?;
    bind_source_port(&socket, config.bind_address)?;
    configure_multicast(&socket, config)?;
    Ok(UdpSocket::from_std(socket.into())?)
}

fn bind_source_port(socket: &Socket, address: SocketAddr) -> Result<()> {
    if address.port() != 0 {
        socket.bind(&address.into())?;
        return Ok(());
    }

    // §11.2, 4.DoIP-135 and Table 48: active requests originate in the IANA
    // dynamic range. Binding port zero can select a lower port on some OSes.
    const FIRST_DYNAMIC_PORT: u32 = 49152;
    const DYNAMIC_PORT_COUNT: u32 = 16384;
    let start = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        % DYNAMIC_PORT_COUNT;
    for offset in 0..DYNAMIC_PORT_COUNT {
        let mut candidate = address;
        candidate.set_port((FIRST_DYNAMIC_PORT + (start + offset) % DYNAMIC_PORT_COUNT) as u16);
        match socket.bind(&candidate.into()) {
            Ok(()) => return Ok(()),
            // Windows also reports WSAEACCES for excluded ports and sockets
            // held with exclusive access. Try the next dynamic port.
            Err(error)
                if error.kind() == std::io::ErrorKind::AddrInUse
                    || (cfg!(windows) && error.raw_os_error() == Some(10013)) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "no free DoIP dynamic UDP port",
    )
    .into())
}

fn configure_multicast(socket: &Socket, config: &mut DiscoveryConfig) -> Result<()> {
    let SocketAddr::V6(ref mut destination) = config.destination else {
        return Ok(());
    };
    if !destination.ip().is_multicast() {
        return Ok(());
    }
    // §7.4, 8.DoIP-155: IPv6 announcements use ff02::1. The interface index
    // selects the link for multicast membership, transmission, and later connects.
    let index = config
        .interface_index
        .filter(|index| *index != 0)
        .ok_or_else(|| {
            Error::InvalidConfig("IPv6 multicast requires a nonzero interface_index".into())
        })?;
    if destination.scope_id() != 0 && destination.scope_id() != index {
        return Err(Error::InvalidConfig("IPv6 scope ID and interface_index differ".into()).into());
    }
    socket.set_multicast_if_v6(index)?;
    socket.join_multicast_v6(destination.ip(), index)?;
    destination.set_scope_id(index);
    Ok(())
}
