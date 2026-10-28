"""Blocking XCP CAN and Ethernet transports."""

from typing import Self

from ...protocols import xcp as _xcp
from ...protocols.xcp import XCPCanConfig
from .._base import Resource
from .._runtime import call, run
from ..adapters import CanAdapter


class XCPTransport(Resource[_xcp.XCPTransport]):
    """Packet I/O retained by one protocol client, with shared lifetime."""

    @property
    def max_tx_packet_size(self) -> int:
        """Largest outgoing packet, excluding headers."""
        return self._async.max_tx_packet_size

    @property
    def max_rx_packet_size(self) -> int:
        """Largest incoming packet, excluding headers."""
        return self._async.max_rx_packet_size


class XCPOnCAN(XCPTransport):
    """Retain the adapter and subscribe to the configured receive ID."""

    def __init__(self, adapter: CanAdapter, config: XCPCanConfig) -> None:
        self.config = config
        super().__init__(call(_xcp.XCPOnCAN, adapter._async, config))


class XCPOnTCP(XCPTransport):
    """XCP packet framing over a TCP byte stream."""

    @classmethod
    def connect(cls, host: str, port: int = 5555, timeout: float = 1.0) -> Self:
        """Open TCP before constructing an XCPClient."""
        return cls(run(_xcp.XCPOnTCP.connect, host, port, timeout))


class XCPOnUDP(XCPTransport):
    """XCP packet framing over UDP datagrams."""

    @classmethod
    def connect(
        cls,
        host: str,
        port: int = 5555,
        timeout: float = 1.0,
        *,
        local_addr: tuple[str, int] | None = None,
    ) -> Self:
        """Open UDP with an optional local (host, port), matching the async API."""
        return cls(run(_xcp.XCPOnUDP.connect, host, port, timeout, local_addr=local_addr))
