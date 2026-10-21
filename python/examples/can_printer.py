"""Print frames from the first available adapter. Requires CAN hardware or vcan0."""

import asyncio

from automotive.adapters import Adapter


async def main() -> None:
    """Discover an adapter and print frames until interrupted."""
    async with Adapter() as adapter, adapter.recv() as frames:
        async for frame in frames:
            print(frame)


if __name__ == "__main__":
    asyncio.run(main())
