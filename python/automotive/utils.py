"""Utilities for displaying ECU memory."""


def hexdump(data: bytes | bytearray | memoryview, address: int = 0, *, address_granularity: int = 1) -> str:
    """Format bytes in 16-byte rows with addresses, hex bytes and ASCII.

    ``address`` is the ECU address displayed for the first byte.
    ``address_granularity`` is bytes per ECU address, as negotiated by XCP.
    """
    if address_granularity <= 0:
        raise ValueError("address_granularity must be positive")
    data = bytes(data)
    lines = []
    for offset in range(0, len(data), 16):
        chunk = data[offset : offset + 16]
        hex_data = chunk.hex(" ")
        ascii_data = "".join(chr(byte) if 32 <= byte < 127 else "." for byte in chunk)
        lines.append(f"{address + offset // address_granularity:08x}  {hex_data:<47}  |{ascii_data}|")
    return "\n".join(lines)
