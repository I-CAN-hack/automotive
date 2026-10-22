"""Synchronous CAN, ISO-TP, and UDS with the same Rust implementation.

Import blocking adapter, transport, and client classes from automotive.sync.
Constructors open adapters immediately. Use ordinary method calls and for loops.
Context managers and explicit close() remain available. Configuration objects,
CAN frames, enum values, result types, and exceptions are shared with the async
API. One background event loop runs the async façade and shuts down at process
exit. Hardware and subscriptions are released when their last owner is dropped;
explicit close() and with blocks support deterministic cleanup.

These calls block the calling thread. Use automotive.adapters,
automotive.transports.isotp, and automotive.protocols.uds in asyncio programs.
"""

from ..adapters import (
    AdapterConfig,
    J2534Config,
    PandaConfig,
    PeakConfig,
    SocketCanConfig,
    VectorConfig,
    available_adapters,
)
from ..protocols.uds import UDSConfig
from ..transports.isotp import IsoTPConfig
from ._runtime import Receiver
from .adapters import Adapter, J2534Adapter, PandaAdapter, PeakAdapter, SocketCanAdapter, VectorAdapter
from .transports import IsoTPAdapter, J2534Device, J2534NativeIsoTpTransport
from .uds import UDSClient

__all__ = [
    "Adapter",
    "AdapterConfig",
    "IsoTPAdapter",
    "IsoTPConfig",
    "J2534Adapter",
    "J2534Config",
    "J2534Device",
    "J2534NativeIsoTpTransport",
    "PandaAdapter",
    "PandaConfig",
    "PeakAdapter",
    "PeakConfig",
    "Receiver",
    "SocketCanAdapter",
    "SocketCanConfig",
    "UDSClient",
    "UDSConfig",
    "VectorAdapter",
    "VectorConfig",
    "available_adapters",
]
