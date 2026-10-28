"""Rust-backed XCP over raw CAN/CAN FD and Ethernet TCP/UDP.

ASAM XCP 1.5 sections 7.5.1 and 7.5.2 define command encoding. All byte
ordering, alignment, framing, serialization, and deadlines are handled by Rust.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from .. import _native
from .._base import AsyncResource, Buffer, buffer_bytes
from ._xcp_enums import Command, ConnectMode, ErrorCode, IdentificationType, Resource
from ._xcp_transport import XCPCanConfig, XCPOnCAN, XCPOnTCP, XCPOnUDP, XCPTransport


@dataclass(frozen=True, slots=True, kw_only=True)
class XCPConfig:
    """Command deadline in seconds, including transmission and reception.

    Timeout must be finite and positive. Commands are never automatically
    retried. Cancelling a command before it acquires the session sends nothing.
    Cancelling an active exchange requires an explicit synchronize() afterwards.
    """

    timeout: float = _native.xcp_defaults()["timeout"]

    def __post_init__(self) -> None:
        _native.validate_xcp(self)


@dataclass(frozen=True, slots=True)
class ConnectInfo:
    """Negotiated limits. Version fields contain major versions only.

    address_granularity is bytes per ECU address. max_upload_size is the
    largest byte count accepted by a single SHORT_UPLOAD request.
    """

    resource: int
    comm_mode_basic: int
    byte_order: Literal["little", "big"]
    address_granularity: int
    max_cto: int
    max_dto: int
    protocol_version: int
    transport_version: int
    max_upload_size: int

    @property
    def slave_block_mode(self) -> bool:
        """Whether the ECU advertises slave block transfer support."""
        return bool(self.comm_mode_basic & 0x40)


@dataclass(frozen=True, slots=True)
class Status:
    """Session and resource protection masks returned by GET_STATUS."""

    session_status: int
    resource_protection_status: int
    session_configuration_id: int


@dataclass(frozen=True, slots=True)
class CommunicationModeInfo:
    """Optional communication modes and limits returned by the ECU."""

    comm_mode_optional: int
    max_bs: int
    min_st: int
    queue_size: int
    driver_version: int


@dataclass(frozen=True, slots=True)
class Identification:
    """GET_ID result; mode 0 sets MTA for subsequent UPLOAD commands."""

    mode: int
    length: int
    data: bytes


@dataclass(frozen=True, slots=True)
class Checksum:
    """Raw XCP checksum type and ECU-calculated value."""

    checksum_type: int
    checksum: int


@dataclass(frozen=True, slots=True)
class Seed:
    """GET_SEED segment; length includes this and all remaining seed bytes."""

    length: int
    data: bytes


class XCPClient(AsyncResource[_native._XCP]):
    """Persistent XCP client retaining its CAN adapter or Ethernet socket.

    Construct with XCPOnCAN, XCPOnTCP or XCPOnUDP, then call connect()
    before other commands. One client serializes its command exchanges.
    After cancellation, a timeout, or a malformed response, call synchronize(); an interrupted
    TCP transmission requires closing and reopening the client. Negative
    responses raise XCPResponseError with raw command, code, and data fields.

    Optional memory commands require ECU support and sufficient resource
    access. Memory sizes are bytes, aligned to the negotiated address
    granularity; addresses count ECU elements. No memory write or programming
    command is sent by connection or read commands. Each method sends one
    command; callers handle transfer loops and segmented seed/key exchanges.
    Closing waits for an active exchange to finish or reach its deadline.
    """

    def __init__(self, transport: XCPTransport, config: XCPConfig | None = None) -> None:
        self.transport = transport
        self.config = config or XCPConfig()
        self.connection_info: ConnectInfo | None = None
        super().__init__(_native._XCP(transport._native, self.config))

    async def connect(self, mode: ConnectMode | int = ConnectMode.NORMAL) -> ConnectInfo:
        """CONNECT in normal mode (0), or the ECU-defined mode (1)."""
        self.connection_info = ConnectInfo(**await self._native.call("connect", (mode,)))
        return self.connection_info

    async def disconnect(self) -> None:
        """Send DISCONNECT and clear the negotiated session after acknowledgement."""
        await self._native.call("disconnect", ())
        self.connection_info = None

    async def synchronize(self) -> None:
        """Recover synchronization by waiting for SYNCH's ERR_CMD_SYNCH reply."""
        await self._native.call("synchronize", ())

    async def get_status(self) -> Status:
        """Read session state and resource protection without changing the ECU."""
        return Status(**await self._native.call("get_status", ()))

    async def get_comm_mode_info(self) -> CommunicationModeInfo:
        """Read optional communication capabilities."""
        return CommunicationModeInfo(**await self._native.call("get_comm_mode_info", ()))

    async def get_id(self, id_type: IdentificationType | int = IdentificationType.ASCII) -> Identification:
        """GET_ID; zero requests ASCII identification, other values are ECU-dependent."""
        return Identification(**await self._native.call("get_id", (id_type,)))

    async def get_seed(self, mode: int, resource: Resource | int) -> Seed:
        """GET_SEED: read one segment. Mode 0 starts; mode 1 continues."""
        return Seed(**await self._native.call("get_seed", (mode, resource)))

    async def unlock(self, length: int, key: Buffer) -> int:
        """UNLOCK: send one segment; length includes all remaining key bytes."""
        return await self._native.call("unlock", (length, buffer_bytes(key)))

    async def set_mta(self, address: int, address_extension: int = 0) -> None:
        """SET_MTA: set the ECU memory transfer address."""
        await self._native.call("set_mta", (address, address_extension))

    async def upload(self, size: int) -> bytes:
        """UPLOAD: read one response at MTA; size is bytes aligned to AG."""
        return await self._native.call("upload", (size,))

    async def short_upload(self, address: int, size: int, address_extension: int = 0) -> bytes:
        """Read one response, at most connection_info.max_upload_size bytes.

        Size is in bytes and must be aligned to address_granularity. Rust
        encodes the element count, address extension, address, and alignment.
        """
        return await self._native.call("short_upload", (address, size, address_extension))

    async def download(self, data: Buffer) -> None:
        """DOWNLOAD: write one packet at MTA within the negotiated MAX_CTO."""
        await self._native.call("download", (buffer_bytes(data),))

    async def build_checksum(self, block_size: int) -> Checksum:
        """Calculate a checksum at MTA; block_size counts ECU address elements."""
        return Checksum(**await self._native.call("build_checksum", (block_size,)))


__all__ = [
    "Checksum",
    "Command",
    "CommunicationModeInfo",
    "ConnectInfo",
    "ConnectMode",
    "ErrorCode",
    "Identification",
    "IdentificationType",
    "Resource",
    "Seed",
    "Status",
    "XCPCanConfig",
    "XCPClient",
    "XCPConfig",
    "XCPOnCAN",
    "XCPOnTCP",
    "XCPOnUDP",
    "XCPTransport",
]
