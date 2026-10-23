"""Blocking software ISO-TP on top of any supported CAN adapter."""

from __future__ import annotations

from typing import Self

from ...transports import isotp as _isotp
from ...transports.isotp import IsoTPConfig
from .._runtime import call
from ..adapters import CanAdapter
from ..can import Identifier
from . import Transport


class IsoTP(Transport):
    """A blocking complete-message ISO-TP transport retaining its CAN adapter.

    Args:
        adapter: Open synchronous CAN adapter.
        config: Shared ISO-TP configuration; only tx_id is required.

    Closing this transport leaves the CAN adapter open. Closing the adapter
    invalidates this transport and its clients. Drop or close raw ISO-TP receivers
    before issuing UDS requests on the same transport.
    Sending an empty or oversized payload raises ValueError. Framing and peer
    rejection failures raise IsoTPError; receive and transmit deadlines raise
    TimeoutError.
    """

    _async: _isotp.IsoTP

    def __init__(self, adapter: CanAdapter, config: IsoTPConfig) -> None:
        super().__init__(call(_isotp.IsoTP, adapter._async, config))

    @property
    def config(self) -> IsoTPConfig:
        """The shared ISO-TP configuration used by this transport."""
        return self._async.config

    @classmethod
    def from_id(cls, adapter: CanAdapter, tx_id: int | Identifier) -> Self:
        """Create a transport with defaults and a Rust-derived receive ID.

        Args:
            adapter: Open synchronous CAN adapter.
            tx_id: Integer or explicit transmit identifier.
        """
        return cls(adapter, IsoTPConfig(tx_id=tx_id))


__all__ = ["IsoTP", "IsoTPConfig"]
