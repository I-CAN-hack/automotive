"""XCP-specific CAN and Ethernet transport implementations."""

from dataclasses import dataclass
from typing import Self

from .. import _native
from .._base import AsyncResource
from ..adapters import CanAdapter
from ..can import Identifier


class XCPTransport(AsyncResource[_native._XCPTransport]):
    """Own complete-packet I/O and framing; attach one XCP client at a time.

    Closing stops new commands and waits for the active exchange or its timeout.
    Rust implementations provide limits, send, receive and optional recovery.
    """

    @property
    def max_tx_packet_size(self) -> int:
        """Largest outgoing packet, excluding transport headers."""
        return self._native.max_tx_packet_size

    @property
    def max_rx_packet_size(self) -> int:
        """Largest incoming packet, excluding transport headers."""
        return self._native.max_rx_packet_size


@dataclass(frozen=True, slots=True, kw_only=True)
class XCPCanConfig:
    """CAN addressing and frame configuration.

    fd selects CAN FD. brs=None follows the adapter setting. max_dlen=None
    selects eight bytes for CAN or 64 for FD. padding=None disables optional
    fill; FD still rounds to a valid DLC. SocketCAN timing is configured by
    the OS. Integers or explicit standard/extended Identifiers are accepted.
    """

    tx_id: int | Identifier
    rx_id: int | Identifier
    bus: int = 0
    fd: bool = False
    brs: bool | None = None
    padding: int | None = 0
    max_dlen: int | None = None

    def __post_init__(self) -> None:
        _native.validate_xcp_can(self)


class XCPOnCAN(XCPTransport):
    """Packet transport retaining an adapter and its own receive subscription."""

    def __init__(self, adapter: CanAdapter, config: XCPCanConfig) -> None:
        self.config = config
        super().__init__(_native.open_xcp_can(adapter._native, config))


class XCPOnTCP(XCPTransport):
    """Reassemble complete XCP packets from TCP, retaining partial frames."""

    @classmethod
    async def connect(cls, host: str, port: int = 5555, timeout: float = 1.0) -> Self:
        """Open TCP; an XCPClient separately negotiates the XCP session."""
        return cls(await _native.open_xcp_tcp(host, port, timeout))


class XCPOnUDP(XCPTransport):
    """XCP UDP packets from a fixed ECU peer, with datagram boundary checks."""

    @classmethod
    async def connect(
        cls,
        host: str,
        port: int = 5555,
        timeout: float = 1.0,
        *,
        local_addr: tuple[str, int] | None = None,
    ) -> Self:
        """Open UDP, optionally binding a local (host, port) before connecting.

        None selects an ephemeral local port. XCPClient establishes the session.
        """
        return cls(await _native.open_xcp_udp(host, port, timeout, local_addr))
