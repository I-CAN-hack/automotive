"""Named wire constants exported by Rust, represented as Python IntEnums."""

import re
from enum import IntEnum
from typing import Any, cast

from . import _native


def _constant_name(name: str) -> str:
    name = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name).upper()


def _make_enum(name: str) -> type[IntEnum]:
    members = _native.enum_values()[name]
    result = cast(Any, IntEnum)(name, {_constant_name(key): value for key, value in members.items()}, module=__name__)
    result.__doc__ = (
        f"Named {name} wire values from Rust.\n\n"
        "Members are integers and may be passed wherever a raw code is accepted. "
        "Unknown codes remain raw integers in protocol results. Iteration visits "
        "only known values; constructing this enum with an unknown value raises ValueError."
    )
    for member in result:
        member.__doc__ = f"{member.name}: wire value 0x{member.value:x}."
    return result


ServiceIdentifier = _make_enum("ServiceIdentifier")
DataIdentifier = _make_enum("DataIdentifier")
SessionType = _make_enum("SessionType")
ResetType = _make_enum("ResetType")
SecurityAccessType = _make_enum("SecurityAccessType")
RoutineControlType = _make_enum("RoutineControlType")
ReportType = _make_enum("ReportType")
RoutineIdentifier = _make_enum("RoutineIdentifier")
NegativeResponseCode = _make_enum("NegativeResponseCode")
DTCFormatIdentifier = _make_enum("DTCFormatIdentifier")

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
