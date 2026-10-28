"""XCP wire enums generated from the Rust XCP definitions."""

from .. import _native
from .._enums import make_enum

_VALUES = _native.xcp_enum_values()

Command = make_enum("Command", _VALUES["Command"], module="automotive.protocols.xcp")
Resource = make_enum("Resource", _VALUES["Resource"], module="automotive.protocols.xcp", flags=True)
ConnectMode = make_enum("ConnectMode", _VALUES["ConnectMode"], module="automotive.protocols.xcp")
IdentificationType = make_enum("IdentificationType", _VALUES["IdentificationType"], module="automotive.protocols.xcp")
ErrorCode = make_enum("ErrorCode", _VALUES["ErrorCode"], module="automotive.protocols.xcp")

__all__ = ["Command", "ConnectMode", "ErrorCode", "IdentificationType", "Resource"]
