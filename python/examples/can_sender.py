"""Send one Classic CAN frame through the first available adapter."""

import asyncio

from automotive.adapters import Adapter
from automotive.can import Frame


async def main() -> None:
    """Discover an adapter and send a frame."""
    adapter = Adapter()
    frame = Frame(id=0x123, data=b"\x01\x02\x03\x04")
    await adapter.send(frame)
    print("Sent", frame)


if __name__ == "__main__":
    asyncio.run(main())
