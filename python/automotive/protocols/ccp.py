"""Rust-backed CCP 2.1 memory transfers on Classical CAN.

Wire encoding follows CCP 2.1 sections 7 and 12.1–12.9, 12.15, 12.28.
Configure the ECU's memory address byte order before connecting.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from .. import _native
from .._base import AsyncResource, Buffer, buffer_bytes
from ..adapters import CanAdapter
from ..can import Identifier
from ._ccp_enums import Command, ErrorCode, Resource


@dataclass(frozen=True, slots=True, kw_only=True)
class CCPConfig:
    """Command deadline in seconds and ECU memory address byte order.

    CCP does not negotiate byte order. Use the ECU description or a known
    working trace. Station addresses are always little endian. No automatic
    command retries are performed.
    """

    timeout: float = _native.ccp_defaults()["timeout"]
    byte_order: Literal["little", "big"] = _native.ccp_defaults()["byte_order"]

    def __post_init__(self) -> None:
        _native.validate_ccp(self)


@dataclass(frozen=True, slots=True, kw_only=True)
class CCPCanConfig:
    """CRO/DTO identifiers and adapter bus for Classical CAN."""

    tx_id: int | Identifier
    rx_id: int | Identifier
    bus: int = 0

    def __post_init__(self) -> None:
        _native.validate_ccp_can(self)


@dataclass(frozen=True, slots=True)
class ProtocolVersion:
    """ECU protocol version returned by GET_CCP_VERSION."""

    major: int
    minor: int


@dataclass(frozen=True, slots=True)
class Identification:
    """EXCHANGE_ID result; UPLOAD reads the identification at MTA0."""

    length: int
    data_type: int
    resource_availability: int
    resource_protection: int


@dataclass(frozen=True, slots=True)
class Seed:
    """Four seed bytes; protected=False means UNLOCK is unnecessary."""

    protected: bool
    data: bytes


@dataclass(frozen=True, slots=True)
class MemoryAddress:
    """Post-incremented MTA0 returned by DNLOAD or DNLOAD_6."""

    address: int
    address_extension: int


class CCPTransport(AsyncResource[_native._CCPTransport]):
    """Exclusive packet transport; attach one CCP client.

    Closing waits for the active command to finish or reach its deadline.
    """


class CCPOnCAN(CCPTransport):
    """Retain the CAN adapter and subscribe before the first command."""

    def __init__(self, adapter: CanAdapter, config: CCPCanConfig) -> None:
        self.config = config
        super().__init__(_native.open_ccp_can(adapter._native, config))


class CCPClient(AsyncResource[_native._CCP]):
    """Serialized CCP session with one command per method call.

    Connect directly to a known station. UPLOAD/DNLOAD operate on MTA0;
    SHORT_UP leaves MTA0 unchanged. Sizes count bytes. Callers implement
    transfer loops and seed/key algorithms. Optional commands require ECU
    support and sufficient access privileges.

    After an active command is cancelled, times out, or receives a malformed
    response, connect() again and re-establish MTA and privileges. The previous
    command may have executed. Negative responses raise CCPResponseError with
    command, code, and data. Closing releases the transport after the active
    command; disconnect() separately ends the ECU session.
    """

    def __init__(self, transport: CCPTransport, config: CCPConfig | None = None) -> None:
        self.transport = transport
        self.config = config or CCPConfig()
        super().__init__(_native._CCP(transport._native, self.config))

    async def connect(self, station_address: int = 0) -> None:
        """CONNECT to a known station; station addresses are always little endian."""
        await self._native.call("connect", (station_address,))

    async def disconnect(self, temporary: bool = False) -> None:
        """Disconnect temporarily or terminate the calibration session (default)."""
        await self._native.call("disconnect", (temporary,))

    async def get_ccp_version(self, major: int = 2, minor: int = 1) -> ProtocolVersion:
        """Exchange protocol versions before EXCHANGE_ID."""
        return ProtocolVersion(**await self._native.call("get_ccp_version", (major, minor)))

    async def exchange_id(self, master_id: Buffer = b"") -> Identification:
        """Send up to six optional master ID bytes; set MTA0 to the ECU ID."""
        return Identification(**await self._native.call("exchange_id", (buffer_bytes(master_id),)))

    async def get_seed(self, resource: Resource | int) -> Seed:
        """Request seed bytes for one resource: CAL=1, DAQ=2 or PGM=0x40."""
        return Seed(**await self._native.call("get_seed", (resource,)))

    async def unlock(self, key: Buffer) -> int:
        """Send 1–6 key bytes and return the current privilege resource mask."""
        return await self._native.call("unlock", (buffer_bytes(key),))

    async def set_mta(self, address: int, address_extension: int = 0, mta: int = 0) -> None:
        """Set MTA0 for memory transfers, or MTA1 for ECU memory moves."""
        await self._native.call("set_mta", (address, address_extension, mta))

    async def upload(self, size: int) -> bytes:
        """Read 1–5 bytes at MTA0 and post-increment MTA0 by size."""
        return await self._native.call("upload", (size,))

    async def short_upload(self, address: int, size: int, address_extension: int = 0) -> bytes:
        """SHORT_UP reads 1–5 bytes at address without changing MTA0."""
        return await self._native.call("short_upload", (address, size, address_extension))

    async def download(self, data: Buffer) -> MemoryAddress:
        """DNLOAD writes 1–5 bytes at MTA0; return its post-incremented address."""
        return MemoryAddress(**await self._native.call("download", (buffer_bytes(data),)))

    async def download6(self, data: Buffer) -> MemoryAddress:
        """Optional DNLOAD_6 writes exactly six bytes at MTA0."""
        return MemoryAddress(**await self._native.call("download6", (buffer_bytes(data),)))


__all__ = [
    "CCPCanConfig",
    "CCPClient",
    "CCPConfig",
    "CCPOnCAN",
    "CCPTransport",
    "Command",
    "ErrorCode",
    "Identification",
    "MemoryAddress",
    "ProtocolVersion",
    "Resource",
    "Seed",
]
