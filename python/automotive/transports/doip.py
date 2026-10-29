"""DoIP discovery and UDS transport over IPv4 or IPv6, implemented in Rust.

``await discover()`` triggers vehicle identification with no configuration.
Connect to a result with ``await vehicle.connect()`` or use
``await DoIP.connect("192.168.0.10", 0x1000)`` for a known ECU.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Self

from .. import _native
from .._base import AsyncResource
from . import Transport


@dataclass(frozen=True, slots=True, kw_only=True)
class DoIPConfig:
    """Optional TCP overrides; the target ECU address is supplied to connect().

    ``local_address`` selects a source IP/port, e.g. ``192.168.0.2:0`` or
    ``[fe80::2%7]:0``. Omit it to follow OS routing. IPv6 link-local endpoints
    need their numeric interface scope. Default tester address is 0x0E00.
    ``protocol_version`` supports 2, 3, and 4; 2 favors deployed-ECU compatibility.
    Timeouts are seconds; activation and ACK waits default to ISO's two seconds.
    ``activation_oem`` is an optional four-byte manufacturer record.
    ``max_payload_size`` includes logical addresses and bounds peer allocation.
    TLS and manufacturer-specific authentication are not implemented.
    """

    source_address: int = 0x0E00
    protocol_version: int = 2
    activation_type: int = 0
    activation_oem: bytes | None = None
    local_address: str | None = None
    connect_timeout: float = 2.0
    activation_timeout: float = 2.0
    ack_timeout: float = 2.0
    max_payload_size: int = 4 * 1024 * 1024


@dataclass(frozen=True, slots=True, kw_only=True)
class DiscoveryConfig:
    """Optional discovery overrides, using IP endpoints with explicit ports.

    Defaults send IPv4 limited broadcast from an ephemeral source port and
    collect responses for two seconds. Set ``bind_address`` to select a local
    source IP; a subnet-directed broadcast destination can select the route on
    a multihomed host. Passive IPv4 reception normally binds ``0.0.0.0:13400``
    to receive announcements from all adapters. IPv6 multicast requires an OS
    interface index, supplied by ``ipv6(index)``. Both work on Windows and Unix.

    ``vin`` (17 raw bytes) and ``eid`` (six bytes) select identification request
    variants and filter replies; they are mutually exclusive. Version 255
    discovers all supported versions. ``timeout`` is seconds. Socket sharing
    is opt-in; Windows may distribute unicast packets among shared listeners.
    """

    bind_address: str = "0.0.0.0:0"
    destination: str = "255.255.255.255:13400"
    interface_index: int | None = None
    protocol_version: int = 255
    vin: bytes | None = None
    eid: bytes | None = None
    timeout: float = 2.0
    reuse_address: bool = False

    @classmethod
    def ipv6(cls, interface_index: int) -> Self:
        """Use ff02::1 multicast on an explicitly selected IPv6 adapter."""
        return cls(
            bind_address="[::]:0",
            destination=f"[ff02::1%{interface_index}]:13400",
            interface_index=interface_index,
        )

    def passive(self) -> Self:
        """Return settings binding announcement port 13400 instead of port zero."""
        from dataclasses import replace

        return replace(self, bind_address=self.bind_address.rpartition(":")[0] + ":13400")


@dataclass(frozen=True, slots=True, kw_only=True)
class VehicleAnnouncement:
    """Vehicle announcement or identification response, preserving raw VIN/GID.

    ``address`` is the UDP sender endpoint, with any IPv6 scope retained.
    Its source port may differ from the TCP diagnostic port. ``logical_address``
    is the announcing entity; a downstream ECU may have another logical address.
    ``vin_gid_sync_status`` is None when the optional field was omitted.
    """

    address: str
    protocol_version: int
    vin: bytes
    logical_address: int
    eid: bytes
    gid: bytes
    further_action: int
    vin_gid_sync_status: int | None

    @property
    def host(self) -> str:
        """IP address, including the scope ID for a link-local IPv6 sender."""
        return self.address.rpartition(":")[0].strip("[]")

    async def connect(self, config: DoIPConfig | None = None, *, port: int = 13400) -> DoIP:
        """Connect to this entity, using its advertised protocol version by default.

        Uses central-security activation when further_action is 0x10. Supply a
        config to override activation, tester address, or advertised version.
        For a downstream ECU, call DoIP.connect(host, target_address) directly.
        """
        config = config or DoIPConfig(
            protocol_version=self.protocol_version,
            activation_type=0xE0 if self.further_action == 0x10 else 0,
        )
        return await DoIP.connect(self.host, self.logical_address, port=port, config=config)


class DoIP(Transport):
    """A routed TCP connection carrying complete UDS PDUs.

    Alive checks are handled even while idle. send() waits for a matching DoIP
    ACK; UDS replies arrive through recv(). Create raw receivers before sending.
    Serialize UDS exchanges on each connection. Reconnect after a transport
    timeout, malformed TCP message, or cancellation with a response outstanding;
    diagnostic requests are never replayed automatically. A subscription that
    falls behind raises DoIPError instead of silently losing responses.
    """

    @classmethod
    async def connect(
        cls, host: str, target_address: int, *, port: int = 13400, config: DoIPConfig | None = None
    ) -> Self:
        """Connect and activate routing; only host and ECU logical address are required.

        Host can be an IPv4/IPv6 address or DNS name. Use a numeric scope on
        link-local IPv6, e.g. ``fe80::1%7``. OS routing selects the adapter unless
        config.local_address binds a source endpoint.
        """
        endpoint = f"[{host}]:{port}" if ":" in host else f"{host}:{port}"
        return cls(await _native.connect_doip(endpoint, target_address, config or DoIPConfig()))


async def discover(config: DiscoveryConfig | None = None) -> list[VehicleAnnouncement]:
    """Trigger identification and collect unique vehicle responses (two seconds by default)."""
    return [VehicleAnnouncement(**value) for value in await _native.discover_doip(config or DiscoveryConfig())]


class Discovery(AsyncResource[_native._DoIPDiscovery]):
    """Bound UDP socket for passive announcements or explicitly triggered requests.

    Use bind() for an active socket, or listen() for passive announcements on
    port 13400. request() sends identification; recv() waits without transmitting.
    Closing wakes pending operations. Context management releases the socket.
    """

    @classmethod
    async def bind(cls, config: DiscoveryConfig | None = None) -> Self:
        """Open a discovery socket before sending; defaults to an ephemeral IPv4 port."""
        return cls(await _native.bind_doip(config or DiscoveryConfig()))

    @classmethod
    async def listen(cls, config: DiscoveryConfig | None = None) -> Self:
        """Listen on announcement port 13400, sending no request; IPv4 by default."""
        return await cls.bind((config or DiscoveryConfig()).passive())

    @property
    def local_address(self) -> str:
        """Bound local IP/port, including the dynamically assigned request port."""
        return self._native.local_address

    async def request(self) -> None:
        """Send the configured all-vehicle, VIN, or EID identification request."""
        await self._native.request()

    async def recv(self) -> VehicleAnnouncement:
        """Receive one announcement indefinitely; cancellation safely resumes later."""
        return VehicleAnnouncement(**await self._native.recv())


__all__ = ["Discovery", "DiscoveryConfig", "DoIP", "DoIPConfig", "VehicleAnnouncement", "discover"]
