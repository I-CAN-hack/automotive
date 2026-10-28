"""Synchronous CAN, ISO-TP, UDS, and XCP with the same Rust implementation.

Import blocking classes from automotive.sync.adapters,
automotive.sync.transports.isotp, and automotive.sync.protocols.uds.
Constructors open adapters immediately. Use ordinary method calls and for loops.
Context managers and explicit close() remain available. Configuration objects,
CAN frames, enum values, result types, and exceptions are shared with the async
API. One background event loop runs the async façade and shuts down at process
exit. Hardware and subscriptions are released when their last owner is dropped;
explicit close() and with blocks support deterministic cleanup.

These calls block the calling thread. Use automotive.adapters,
automotive.transports.isotp, and automotive.protocols.uds in asyncio programs.
"""

from .. import __version__
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
