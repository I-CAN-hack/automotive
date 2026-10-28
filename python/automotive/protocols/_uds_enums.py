"""UDS wire enums generated from the Rust UDS definitions."""

from .. import _native
from .._enums import make_enum

_VALUES = _native.enum_values()

ServiceIdentifier = make_enum("ServiceIdentifier", _VALUES["ServiceIdentifier"], module="automotive.protocols.uds")
DataIdentifier = make_enum("DataIdentifier", _VALUES["DataIdentifier"], module="automotive.protocols.uds")
SessionType = make_enum("SessionType", _VALUES["SessionType"], module="automotive.protocols.uds")
ResetType = make_enum("ResetType", _VALUES["ResetType"], module="automotive.protocols.uds")
SecurityAccessType = make_enum("SecurityAccessType", _VALUES["SecurityAccessType"], module="automotive.protocols.uds")
RoutineControlType = make_enum("RoutineControlType", _VALUES["RoutineControlType"], module="automotive.protocols.uds")
ReportType = make_enum("ReportType", _VALUES["ReportType"], module="automotive.protocols.uds")
RoutineIdentifier = make_enum("RoutineIdentifier", _VALUES["RoutineIdentifier"], module="automotive.protocols.uds")
NegativeResponseCode = make_enum(
    "NegativeResponseCode", _VALUES["NegativeResponseCode"], module="automotive.protocols.uds"
)
DTCFormatIdentifier = make_enum(
    "DTCFormatIdentifier", _VALUES["DTCFormatIdentifier"], module="automotive.protocols.uds"
)

__all__ = [
    "DTCFormatIdentifier",
    "DataIdentifier",
    "NegativeResponseCode",
    "ReportType",
    "ResetType",
    "RoutineControlType",
    "RoutineIdentifier",
    "SecurityAccessType",
    "ServiceIdentifier",
    "SessionType",
]
