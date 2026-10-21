"""Async CAN, ISO-TP, and UDS powered by the automotive Rust crate.

Start with ``adapters.Adapter()``, ``IsoTPAdapter.from_id()``, and
``UDSClient(transport)``. Explicit config objects expose advanced settings.
Hardware resources support ``async with`` and must be closed after use.
"""

from ._native import __version__
from .errors import (
    AdapterError,
    AutomotiveError,
    IsoTPError,
    NegativeResponseError,
    NotSupportedError,
    ProtocolDeviationError,
    TransmissionCancelledError,
    UDSError,
)

__all__ = [
    "AdapterError",
    "AutomotiveError",
    "IsoTPError",
    "NegativeResponseError",
    "NotSupportedError",
    "ProtocolDeviationError",
    "TransmissionCancelledError",
    "UDSError",
    "__version__",
]
