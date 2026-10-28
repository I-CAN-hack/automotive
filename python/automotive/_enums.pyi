"""Generic enum construction; protocol enum types are declared in their own modules."""

from enum import IntEnum, IntFlag

def make_enum(name: str, members: dict[str, int], *, module: str, flags: bool = False) -> type[IntEnum | IntFlag]: ...
