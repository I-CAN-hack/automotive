"""CCP wire enums generated from the Rust CCP definitions."""

from .. import _native
from .._enums import make_enum

_VALUES = _native.ccp_enum_values()
Command = make_enum("Command", _VALUES["Command"], module="automotive.protocols.ccp")
ErrorCode = make_enum("ErrorCode", _VALUES["ErrorCode"], module="automotive.protocols.ccp")
Resource = make_enum("Resource", _VALUES["Resource"], module="automotive.protocols.ccp", flags=True)

__all__ = ["Command", "ErrorCode", "Resource"]
