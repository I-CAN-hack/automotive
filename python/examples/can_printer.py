"""Print frames from the first available adapter. Requires CAN hardware or vcan0."""

import asyncio

from automotive.adapters import CanAdapter


async def main() -> None:
    """Discover an adapter and print frames until interrupted."""
    adapter = CanAdapter()
    stream = adapter.recv()

    async for frame in stream:
        print(frame)


if __name__ == "__main__":
    asyncio.run(main())
