"""Adapter and protocol exceptions shared with the asynchronous API."""

from ..errors import (
    AdapterError,
    AutomotiveError,
    CCPError,
    CCPResponseError,
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
