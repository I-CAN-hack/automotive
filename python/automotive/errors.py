"""Exceptions raised by Rust adapter and protocol operations.

Invalid arguments raise ``ValueError`` (or ``TypeError``/``OverflowError`` for
Python type conversions). Deadlines raise built-in ``TimeoutError``;
disconnection raises ``ConnectionError`` and OS failures raise ``OSError``.
These built-in exceptions are independent of ``AutomotiveError``.

``NegativeResponseError.code`` is the exact integer NRC, including unknown
values. ``ProtocolDeviationError.response`` is the complete response as bytes;
``deviation`` describes the failed requirement and its expected/actual values.
Transmit-queue clears raise ``TransmissionCancelledError``, distinct from
Python task cancellation (``asyncio.CancelledError``).
"""

from ._native import (
    AdapterError,
    AutomotiveError,
    CCPError,
    CCPResponseError,
    DoIPError,
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
    "CCPError",
    "CCPResponseError",
    "DoIPError",
    "IsoTPError",
    "NegativeResponseError",
    "NotSupportedError",
    "ProtocolDeviationError",
    "TransmissionCancelledError",
    "UDSError",
    "XCPError",
    "XCPResponseError",
    "XCPTransportError",
]
