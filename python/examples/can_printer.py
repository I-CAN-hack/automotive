"""Print frames from the first available adapter. Requires CAN hardware or vcan0."""

import asyncio

from automotive.adapters import Adapter


async def main() -> None:
    """Discover an adapter and print frames until interrupted."""
    adapter = Adapter()
    async for frame in adapter.recv():
        print(frame)


if __name__ == "__main__":
    asyncio.run(main())
