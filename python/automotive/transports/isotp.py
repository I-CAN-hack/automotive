"""Software ISO-TP on top of any supported CAN adapter.

All framing, flow control, separation timing, and validation run in Rust.
Create a receive subscription before sending when exchanging raw PDUs.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Self

from .. import _native
from .._base import AsyncResource, Buffer, Receiver, buffer_bytes
from ..adapters import Adapter
from ..can import Identifier

_DEFAULTS = _native.defaults()


@dataclass(frozen=True, slots=True, kw_only=True)
class IsoTPConfig:
    """Reusable ISO-TP settings, validated and resolved by Rust.

    Args:
        tx_id: Required transmit ID, integer or explicit Identifier.
        rx_id: Receive ID; None derives standard TX+8 or swaps the low two
            bytes of an extended ID. These are convenience mappings.
        rx_id_offset: Alternative standard-ID offset, exclusive with rx_id.
        bus: CAN bus index, default zero.
        fd: Select CAN FD, default False.
        padding: Optional padding byte, default 0xCC. None disables optional
            padding; FD still pads to a valid DLC.
        timeout: Per-stage transport deadline in seconds, default 1.0. Covers
            CAN confirmation, flow control, consecutive frames, and idle receive.
        separation_time_min: Transmit STmin override in seconds; None follows
            the peer's advertised value.
        ext_address: Optional transmit and default receive address byte.
        rx_ext_address: Alternate receive address byte; requires ext_address.
        max_dlen: Transmit data length including the address byte. None selects
            8 for Classic or 64 for FD. Classic requires 8; FD also permits
            12, 16, 20, 24, 32, and 48.
        block_size: Advertised receive block size, default zero (unlimited).
        receive_separation_time_min: Advertised receive STmin in seconds,
            default zero; rounded up to a wire value and limited to 0.127.

    Raises:
        ValueError: Rust rejects an ID mapping, length, or timing setting.

    Durations must be finite and nonnegative; timeout must be greater than zero.
    Use resolved_rx_id to inspect the derived receive identifier.
    """

    tx_id: int | Identifier
    rx_id: int | Identifier | None = None
    rx_id_offset: int | None = None
    bus: int = _DEFAULTS["bus"]
    fd: bool = _DEFAULTS["fd"]
    padding: int | None = _DEFAULTS["padding"]
    timeout: float = _DEFAULTS["timeout"]
    separation_time_min: float | None = None
    ext_address: int | None = None
    rx_ext_address: int | None = None
    max_dlen: int | None = None
    block_size: int = _DEFAULTS["block_size"]
    receive_separation_time_min: float = _DEFAULTS["receive_separation_time_min"]

    def __post_init__(self) -> None:
        _native.validate_isotp(self)

    @property
    def resolved_rx_id(self) -> Identifier:
        """The explicit or Rust-derived receive ID, preserving its format."""
        value, extended = _native.validate_isotp(self)
        return Identifier(value, extended)


class IsoTPAdapter(AsyncResource[_native._Transport]):
    """A complete-message ISO-TP transport retaining its CAN adapter.

    Args:
        adapter: Open CAN Adapter.
        config: Explicit ISO-TP configuration; only its tx_id is required.

    Closing this transport ends its operations without closing the shared CAN
    adapter. Closing the adapter invalidates this transport and its UDS clients.
    Raw receivers must be closed before issuing UDS requests on this transport,
    because both consume PDUs and may send flow-control frames.
    """

    def __init__(self, adapter: Adapter, config: IsoTPConfig) -> None:
        if not isinstance(config, IsoTPConfig):
            raise TypeError("config must be an IsoTPConfig")
        self.config = config
        super().__init__(_native._Transport(adapter._native, config))

    @classmethod
    def from_id(cls, adapter: Adapter, tx_id: int | Identifier) -> Self:
        """Create a transport using default configuration and a derived RX ID.

        Args:
            adapter: Open CAN adapter.
            tx_id: Integer or explicit transmit identifier.

        Raises:
            ValueError: The transmit or derived receive ID is out of range.
        """
        return cls(adapter, IsoTPConfig(tx_id=tx_id))

    async def send(self, data: Buffer) -> None:
        """Transmit one nonempty PDU, letting Rust segment it as needed.

        Args:
            data: Bytes, bytearray, or contiguous byte-oriented memoryview.

        Raises:
            ValueError: Payload is empty or exceeds the protocol limit.
            TimeoutError: Transmit confirmation or required flow control timed out.
            IsoTPError: The peer rejected the transfer or framing failed.
        """
        await self._native.send(buffer_bytes(data))

    def recv(self) -> Receiver[bytes]:
        """Subscribe immediately to reassembled PDUs, returned as bytes.

        Returns:
            An async iterator with aclose and async context management.

        Raises:
            TimeoutError: Raised while iterating if an ISO-TP receive stage expires.
            IsoTPError: Raised while iterating for receive protocol failures.

        Subscribe before send. Closing this receiver releases the subscription;
        a timeout does not destroy it, so another anext may continue reception.
        """
        return Receiver(self._native.recv(), lambda value: value)


__all__ = ["IsoTPAdapter", "IsoTPConfig"]
