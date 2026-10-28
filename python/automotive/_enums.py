"""Shared enum construction; protocol definitions live in their own modules."""

import re
from enum import IntEnum, IntFlag
from typing import Any, cast


def _constant_name(name: str) -> str:
    name = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name).upper()


def make_enum(name: str, members: dict[str, int], *, module: str, flags: bool = False) -> type[IntEnum] | type[IntFlag]:
    base = IntFlag if flags else IntEnum
    result = cast(Any, base)(name, {_constant_name(key): value for key, value in members.items()}, module=module)
    result.__doc__ = (
        f"Named {name} wire values from Rust.\n\n"
        "Members are integers and may be passed wherever a raw code is accepted. "
        + (
            "Flags may be combined; unknown bits are preserved."
            if flags
            else "Unknown codes remain raw integers in protocol results. Iteration visits "
            "only known values; constructing this enum with an unknown value raises ValueError."
        )
    )
    for member in result:
        member.__doc__ = f"{member.name}: wire value 0x{member.value:x}."
    return result
