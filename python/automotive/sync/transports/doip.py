"""Blocking DoIP discovery and UDS transport, using the same Rust implementation."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Self

from ...transports import doip as _doip
from ...transports.doip import DiscoveryConfig, DoIPConfig
from .._base import Resource
from .._runtime import run
from . import Transport


class DoIP(Transport):
    """Blocking routed TCP connection; alive checks continue in the background.

    send() waits for a DoIP ACK. Subscribe with recv() before raw sends and
    serialize UDS exchanges. Reconnect after transport timeouts, malformed TCP
    messages, or cancellation with a response outstanding. No automatic replay.
    Context management closes the connection and wakes pending operations.
    """

    @classmethod
    def connect(cls, host: str, target_address: int, *, port: int = 13400, config: DoIPConfig | None = None) -> Self:
        """Connect and activate routing, with default tester address 0x0E00.

        Supports IPv4, IPv6, and DNS. Include a numeric scope on link-local IPv6
        hosts (``fe80::1%7``). Optional settings match the async DoIPConfig.
        """
        return cls(run(_doip.DoIP.connect, host, target_address, port=port, config=config))


@dataclass(frozen=True, slots=True, kw_only=True)
class VehicleAnnouncement:
    """Discovered vehicle with raw VIN/EID/GID and a blocking connect helper.

    Fields and host property match the async VehicleAnnouncement. UDP source
    ports are independent of the diagnostic TCP port, which defaults to 13400.
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
        """IP address with any link-local IPv6 scope ID retained."""
        return self.address.rpartition(":")[0].strip("[]")

    def connect(self, config: DoIPConfig | None = None, *, port: int = 13400) -> DoIP:
        """Connect with the advertised version and default activation/tester address."""
        config = config or DoIPConfig(
            protocol_version=self.protocol_version,
            activation_type=0xE0 if self.further_action == 0x10 else 0,
        )
        return DoIP.connect(self.host, self.logical_address, port=port, config=config)


def _announcement(value: _doip.VehicleAnnouncement) -> VehicleAnnouncement:
    from dataclasses import asdict

    return VehicleAnnouncement(**asdict(value))


def discover(config: DiscoveryConfig | None = None) -> list[VehicleAnnouncement]:
    """Trigger identification and collect unique vehicles; defaults to IPv4 and two seconds."""
    return [_announcement(value) for value in run(_doip.discover, config)]


class Discovery(Resource[_doip.Discovery]):
    """Blocking UDP discovery socket; request() triggers, recv() only listens.

    Context management releases the socket. IPv6 multicast requires an explicit
    interface index through DiscoveryConfig.ipv6(index), on Windows and Unix.
    """

    @classmethod
    def bind(cls, config: DiscoveryConfig | None = None) -> Self:
        """Bind before sending; defaults to an ephemeral IPv4 request port."""
        return cls(run(_doip.Discovery.bind, config))

    @classmethod
    def listen(cls, config: DiscoveryConfig | None = None) -> Self:
        """Listen passively on announcement port 13400 without sending requests."""
        return cls(run(_doip.Discovery.listen, config))

    @property
    def local_address(self) -> str:
        """Bound local IP/port, including the assigned request port."""
        return self._async.local_address

    def request(self) -> None:
        """Trigger the configured identification request on this socket."""
        run(self._async.request)

    def recv(self) -> VehicleAnnouncement:
        """Block for one announcement or response, preserving any IPv6 scope ID."""
        return _announcement(run(self._async.recv))


__all__ = ["Discovery", "DiscoveryConfig", "DoIP", "DoIPConfig", "VehicleAnnouncement", "discover"]
