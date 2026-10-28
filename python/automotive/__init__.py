"""Synchronous and asynchronous CAN, ISO-TP, UDS, and XCP powered by Rust.

Start with ``adapters.CanAdapter()``, ``IsoTP.from_id()``, and
``UDSClient(transport)``. Explicit config objects expose advanced settings.
Adapter constructors open hardware synchronously. Sending and receiving are
asynchronous. Hardware is released when its last owner is dropped; ``aclose()``
and ``async with`` support deterministic cleanup.
The corresponding modules under ``automotive.sync`` provide blocking calls,
ordinary with blocks, and receive iterators. Configurations and values are shared.
"""

from importlib.metadata import version as _distribution_version

from .errors import (
    AdapterError,
    AutomotiveError,
    IsoTPError,
    NegativeResponseError,
    NotSupportedError,
    ProtocolDeviationError,
    TransmissionCancelledError,
    UDSError,
    XCPError,
    XCPResponseError,
    XCPTransportError,
)

__version__ = _distribution_version("automotive")

__all__ = [
    "AdapterError",
    "AutomotiveError",
    "IsoTPError",
    "NegativeResponseError",
    "NotSupportedError",
    "ProtocolDeviationError",
    "TransmissionCancelledError",
    "UDSError",
    "XCPError",
    "XCPResponseError",
    "XCPTransportError",
    "__version__",
]
