"""Dump 4 KiB of ECU flash using CCP SHORT_UP."""

import asyncio

from automotive.adapters import CanAdapter
from automotive.protocols.ccp import CCPCanConfig, CCPClient, CCPConfig, CCPOnCAN
from automotive.utils import hexdump


async def main() -> None:
    can = CCPCanConfig(tx_id=0x6EC, rx_id=0x6ED)

    config = CCPConfig(byte_order="little")
    adapter = CanAdapter()

    # Connect to CCP
    client = CCPClient(CCPOnCAN(adapter, can), config)
    await client.connect(station_address=0)

    # Read using SHORT_UP
    start = 0x8000_0000
    data = bytearray()
    while len(data) < 0x1000:
        data.extend(await client.short_upload(start + len(data), 4))

    # Print all data
    print(hexdump(data, start))


if __name__ == "__main__":
    asyncio.run(main())
