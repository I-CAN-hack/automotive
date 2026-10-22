"""Configure a Windows Vector channel for 500 kbit/s CAN and 2 Mbit/s CAN FD."""

import asyncio

from automotive.adapters import VectorAdapter, VectorConfig, available_adapters
from automotive.can.bitrate import BitrateSettings


async def main() -> None:
    """Request initialization access using reusable explicit timing settings."""
    if "vector" not in available_adapters():
        print("This example requires Windows and the Vector XL driver.")
        return
    config = VectorConfig(
        channel=0,
        bitrate=BitrateSettings(
            bitrate=500_000,
            sample_point=0.8,
            data_bitrate=2_000_000,
            data_sample_point=0.8,
        ),
    )
    print("Resolved timing:", config.resolved_bitrate)
    adapter = VectorAdapter(config)
    async for frame in adapter.recv():
        print(frame)


if __name__ == "__main__":
    asyncio.run(main())
