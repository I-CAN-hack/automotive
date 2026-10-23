"""Validated CAN identifiers and Classic, FD, remote, and error frames."""

from __future__ import annotations

from dataclasses import dataclass
from enum import IntEnum
from numbers import Number
from typing import Any, Self

from .. import _native
from .._base import Buffer, buffer_bytes
from .bitrate import BitrateConfig, BitrateSettings, BitTiming


class ErrorFlag(IntEnum):
    """CAN wire error flags; these are not OS-specific error reports."""

    ACTIVE = 0
    PASSIVE = 1


@dataclass(frozen=True, slots=True)
class Identifier:
    """A validated CAN arbitration ID, preserving its wire format.

    Equality with a number compares the raw value, so ``frame.id == 0x123``
    works directly. Equality between identifiers also compares the wire format.

    Args:
        value: Unsigned arbitration ID. Standard IDs fit in 11 bits;
            extended IDs fit in 29 bits.
        is_extended: Select extended format, including for values below 0x800.

    Raises:
        ValueError: The value does not fit the selected wire format.
    """

    value: int
    is_extended: bool = False

    def __post_init__(self) -> None:
        _native.validate_identifier(self.value, self.is_extended)

    @classmethod
    def standard(cls, value: int) -> Self:
        """Construct an explicitly standard 11-bit identifier."""
        return cls(value, False)

    @classmethod
    def extended(cls, value: int) -> Self:
        """Construct an explicitly extended 29-bit identifier."""
        return cls(value, True)

    def __int__(self) -> int:
        """Return the raw wire value."""
        return self.value

    def __index__(self) -> int:
        """Allow this identifier wherever Python accepts an integer index."""
        return self.value

    def __eq__(self, other: object) -> bool:
        if isinstance(other, Identifier):
            return self.value == other.value and self.is_extended == other.is_extended
        if isinstance(other, Number):
            return self.value == other
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self.value)


def _identifier(value: int | Identifier) -> Identifier:
    return value if isinstance(value, Identifier) else Identifier(value, value > 0x7FF)


@dataclass(frozen=True, slots=True, init=False)
class Frame:
    """A CAN frame validated by Rust, with immutable payload bytes.

    Args:
        id: Integer or explicit Identifier; None only for error frames.
        data: Bytes, bytearray, or a contiguous byte-oriented memoryview.
        bus: Bus index, default zero.
        fd: Select CAN FD. None chooses FD for payloads longer than eight bytes.
        brs: Enable FD bitrate switching; requires FD and adapter support.
        esi: Set the FD error-state indicator.
        loopback: Whether this is a transmit-confirmation echo.
        kind: "data", "remote", or "error"; prefer the named constructors.
        dlc: Remote-request DLC (0–15); computed for data frames.
        error_flag: Active/passive flag for error frames only.

    Raises:
        ValueError: Rust rejects the frame length, identifier, or flag combination.

    Classic payloads contain 0–8 bytes. FD payload lengths are 0–8, 12, 16,
    20, 24, 32, 48, or 64, without automatic padding. Error frames can be
    represented but cannot currently be transmitted by adapters.
    """

    id: Identifier | None = None
    data: bytes = b""
    bus: int = 0
    fd: bool = False
    brs: bool = False
    esi: bool = False
    loopback: bool = False
    kind: str = "data"
    dlc: int | None = None
    error_flag: ErrorFlag | None = None

    def __init__(
        self,
        id: int | Identifier | None = None,
        data: Buffer = b"",
        bus: int = 0,
        fd: bool | None = None,
        brs: bool = False,
        esi: bool = False,
        loopback: bool = False,
        kind: str = "data",
        dlc: int | None = None,
        error_flag: ErrorFlag | None = None,
    ) -> None:
        """Copy payload input and construct an immutable, Rust-validated frame."""
        object.__setattr__(self, "id", None if id is None else _identifier(id))
        object.__setattr__(self, "data", buffer_bytes(data))
        object.__setattr__(self, "bus", bus)
        object.__setattr__(self, "fd", fd)
        object.__setattr__(self, "brs", brs)
        object.__setattr__(self, "esi", esi)
        object.__setattr__(self, "loopback", loopback)
        object.__setattr__(self, "kind", kind)
        object.__setattr__(self, "dlc", dlc)
        object.__setattr__(self, "error_flag", error_flag)
        values = _native.validate_frame(self)
        object.__setattr__(self, "fd", values["fd"])
        object.__setattr__(self, "dlc", values["dlc"])

    @classmethod
    def remote(cls, id: int | Identifier, dlc: int = 8, *, bus: int = 0) -> Self:
        """Construct a Classic remote request without a payload.

        Args:
            id: Arbitration ID, inferring format for integer inputs.
            dlc: Four-bit requested DLC. Codes 8–15 request eight bytes.
            bus: Bus index, default zero.
        """
        return cls(id=id, bus=bus, kind="remote", dlc=dlc)

    @classmethod
    def error(cls, flag: ErrorFlag = ErrorFlag.ACTIVE, *, bus: int = 0) -> Self:
        """Represent a wire error flag without an arbitration ID or payload."""
        return cls(bus=bus, kind="error", error_flag=flag)

    @classmethod
    def _from_native(cls, values: dict[str, Any]) -> Self:
        values = dict(values)
        extended = values.pop("extended")
        if values["id"] is not None:
            values["id"] = Identifier(values["id"], extended)
        if values["error_flag"] is not None:
            values["error_flag"] = ErrorFlag(values["error_flag"])
        return cls(**values)


__all__ = ["BitTiming", "BitrateConfig", "BitrateSettings", "ErrorFlag", "Frame", "Identifier"]
