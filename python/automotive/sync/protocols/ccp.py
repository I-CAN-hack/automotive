"""Blocking CCP API forwarding to the Rust-backed asynchronous client."""

from __future__ import annotations

from ..._base import Buffer
from ...protocols import ccp as _ccp
from ...protocols.ccp import (
    CCPConfig,
    Command,
    ErrorCode,
    Identification,
    MemoryAddress,
    ProtocolVersion,
    Resource,
    Seed,
)
from .._base import Resource as _Resource
from .._runtime import call, run
from ..adapters import CanAdapter

__all__ = [
    "CCPClient",
    "CCPConfig",
    "Command",
    "ErrorCode",
    "Identification",
    "MemoryAddress",
    "ProtocolVersion",
    "Resource",
    "Seed",
]


class CCPClient(_Resource[_ccp.CCPClient]):
    """Synchronous CCP; configuration, results and exceptions match the async API.

    After timeout, cancellation or a malformed response, reconnect and
    re-establish MTA and access privileges before continuing.
    The client retains the adapter and subscribes before the first command.
    Closing waits for the active command and releases the subscription without
    closing the shared adapter; disconnect() separately ends the ECU session.
    Closing the adapter invalidates the client.
    """

    def __init__(self, adapter: CanAdapter, config: CCPConfig) -> None:
        super().__init__(call(_ccp.CCPClient, adapter._async, config))

    @property
    def config(self) -> CCPConfig:
        """CAN identifiers, bus, deadline and ECU memory address byte order."""
        return self._async.config

    def connect(self, station_address: int = 0) -> None:
        """CONNECT to a known station; station addresses are always little endian."""
        run(self._async.connect, station_address)

    def disconnect(self, temporary: bool = False) -> None:
        """Disconnect temporarily or terminate the calibration session (default)."""
        run(self._async.disconnect, temporary)

    def get_ccp_version(self, major: int = 2, minor: int = 1) -> ProtocolVersion:
        """Exchange protocol versions before EXCHANGE_ID."""
        return run(self._async.get_ccp_version, major, minor)

    def exchange_id(self, master_id: Buffer = b"") -> Identification:
        """Send up to six optional master ID bytes; set MTA0 to the ECU ID."""
        return run(self._async.exchange_id, master_id)

    def get_seed(self, resource: Resource | int) -> Seed:
        """Request seed bytes for one resource: CAL=1, DAQ=2 or PGM=0x40."""
        return run(self._async.get_seed, resource)

    def unlock(self, key: Buffer) -> int:
        """Send 1–6 key bytes and return the current privilege resource mask."""
        return run(self._async.unlock, key)

    def set_mta(self, address: int, address_extension: int = 0, mta: int = 0) -> None:
        """Set MTA0 for memory transfers, or MTA1 for ECU memory moves."""
        run(self._async.set_mta, address, address_extension, mta)

    def upload(self, size: int) -> bytes:
        """Read 1–5 bytes at MTA0 and post-increment MTA0 by size."""
        return run(self._async.upload, size)

    def short_upload(self, address: int, size: int, address_extension: int = 0) -> bytes:
        """SHORT_UP reads 1–5 bytes at address without changing MTA0."""
        return run(self._async.short_upload, address, size, address_extension)

    def download(self, data: Buffer) -> MemoryAddress:
        """DNLOAD writes 1–5 bytes at MTA0; return its post-incremented address."""
        return run(self._async.download, data)

    def download6(self, data: Buffer) -> MemoryAddress:
        """Optional DNLOAD_6 writes exactly six bytes at MTA0."""
        return run(self._async.download6, data)
