"""Blocking XCP API forwarding to the same Rust-backed asynchronous client."""

from __future__ import annotations

from ..._base import Buffer
from ...protocols import xcp as _xcp
from ...protocols.xcp import (
    Checksum,
    Command,
    CommunicationModeInfo,
    ConnectInfo,
    ConnectMode,
    ErrorCode,
    Identification,
    IdentificationType,
    Resource,
    Seed,
    Status,
    XCPCanConfig,
    XCPConfig,
)
from .._base import Resource as _Resource
from .._runtime import call, run
from ._xcp_transport import XCPOnCAN, XCPOnTCP, XCPOnUDP, XCPTransport

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


class XCPClient(_Resource[_xcp.XCPClient]):
    """Synchronous XCP over CAN, CAN FD, TCP or UDP.

    Configuration, values, exceptions and cancellation behavior match the
    async client. Call connect() before commands. close() releases the local
    transport; disconnect() gracefully ends the ECU session first.
    """

    def __init__(self, transport: XCPTransport, config: XCPConfig | None = None) -> None:
        self.transport = transport
        super().__init__(call(_xcp.XCPClient, transport._async, config))

    @property
    def config(self) -> XCPConfig:
        """Shared command timing settings."""
        return self._async.config

    @property
    def connection_info(self) -> ConnectInfo | None:
        """Capabilities returned by the last successful CONNECT."""
        return self._async.connection_info

    def connect(self, mode: ConnectMode | int = ConnectMode.NORMAL) -> ConnectInfo:
        """CONNECT with normal (0) or user-defined (1) mode."""
        return run(self._async.connect, mode)

    def disconnect(self) -> None:
        """End the XCP session and await its acknowledgement."""
        run(self._async.disconnect)

    def synchronize(self) -> None:
        """Recover after an uncertain command by waiting for ERR_CMD_SYNCH."""
        run(self._async.synchronize)

    def get_status(self) -> Status:
        """Read session and resource protection status."""
        return run(self._async.get_status)

    def get_comm_mode_info(self) -> CommunicationModeInfo:
        """Read optional communication capabilities."""
        return run(self._async.get_comm_mode_info)

    def get_id(self, id_type: IdentificationType | int = IdentificationType.ASCII) -> Identification:
        """GET_ID; mode zero sets MTA for subsequent UPLOAD commands."""
        return run(self._async.get_id, id_type)

    def get_seed(self, mode: int, resource: Resource | int) -> Seed:
        """GET_SEED: read one segment, with mode 0 to start or 1 to continue."""
        return run(self._async.get_seed, mode, resource)

    def unlock(self, length: int, key: Buffer) -> int:
        """UNLOCK: send one segment; length includes all remaining key bytes."""
        return run(self._async.unlock, length, key)

    def set_mta(self, address: int, address_extension: int = 0) -> None:
        """Set MTA; addresses count the ECU's negotiated address elements."""
        run(self._async.set_mta, address, address_extension)

    def upload(self, size: int) -> bytes:
        """UPLOAD: read one response at MTA; size is bytes aligned to AG."""
        return run(self._async.upload, size)

    def short_upload(self, address: int, size: int, address_extension: int = 0) -> bytes:
        """Read one SHORT_UPLOAD response; size is bytes aligned to AG."""
        return run(self._async.short_upload, address, size, address_extension)

    def download(self, data: Buffer) -> None:
        """DOWNLOAD: write one packet at MTA within the negotiated MAX_CTO."""
        run(self._async.download, data)

    def build_checksum(self, block_size: int) -> Checksum:
        """Calculate checksum at MTA; block_size counts ECU elements."""
        return run(self._async.build_checksum, block_size)
