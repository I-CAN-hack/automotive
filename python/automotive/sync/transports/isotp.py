"""Blocking software ISO-TP on top of any supported CAN adapter."""

from __future__ import annotations

from typing import Self

from ..._base import Buffer
from ...transports import isotp as _isotp
from ...transports.isotp import IsoTPConfig
from .._base import Receiver, Resource
from .._runtime import call, run
from ..adapters import Adapter
from ..can import Identifier


class IsoTPAdapter(Resource[_isotp.IsoTPAdapter]):
    """A blocking complete-message ISO-TP transport retaining its CAN adapter.

    Args:
        adapter: Open synchronous CAN adapter.
        config: Shared ISO-TP configuration; only tx_id is required.

    Closing this transport leaves the CAN adapter open. Closing the adapter
    invalidates this transport and its clients. Drop or close raw ISO-TP receivers
    before issuing UDS requests on the same transport.
    """

    def __init__(self, adapter: Adapter, config: IsoTPConfig) -> None:
        super().__init__(call(_isotp.IsoTPAdapter, adapter._async, config))

    @property
    def config(self) -> IsoTPConfig:
        """The shared ISO-TP configuration used by this transport."""
        return self._async.config

    @classmethod
    def from_id(cls, adapter: Adapter, tx_id: int | Identifier) -> Self:
        """Create a transport with defaults and a Rust-derived receive ID.

        Args:
            adapter: Open synchronous CAN adapter.
            tx_id: Integer or explicit transmit identifier.
        """
        return cls(adapter, IsoTPConfig(tx_id=tx_id))

    def send(self, data: Buffer) -> None:
        """Transmit one nonempty PDU, blocking until Rust completes the transfer.

        Args:
            data: Bytes, bytearray, or a contiguous byte-oriented memoryview.

        Raises:
            ValueError: The payload is empty or exceeds the protocol limit.
            TimeoutError: Confirmation or required flow control timed out.
            IsoTPError: The peer rejected the transfer or framing failed.
        """
        run(self._async.send, data)

    def recv(self) -> Receiver[bytes]:
        """Subscribe immediately to complete PDUs as bytes.

        Returns:
            A blocking iterator supporting with and close(). Subscribe before
            sending. A receive timeout raises TimeoutError; next() can resume.
        """
        return Receiver(call(self._async.recv))


__all__ = ["IsoTPAdapter", "IsoTPConfig"]
