"""Read 4 KiB using SET_MTA and UPLOAD over XCP on CAN and CAN FD."""

import asyncio

from automotive.adapters import CanAdapter
from automotive.can import Frame
from automotive.protocols.xcp import XCPCanConfig, XCPClient, XCPOnCAN


async def main() -> None:
    adapter = CanAdapter()

    # Wake ECU
    wake = Frame(id=0x541, data=b"\xff" * 8)
    for _ in range(10):
        await adapter.send(wake)
        await asyncio.sleep(0.1)

    config = XCPCanConfig(tx_id=0x6C3, rx_id=0x6C4, fd=False)
    client = XCPClient(XCPOnCAN(adapter, config))
    info = await client.connect()
    print(f"Connected: {info}")
    chunk_size = info.max_upload_size

    # Read using UPLOAD
    await client.set_mta(0x8000_0000)
    data = bytearray()
    while len(data) < 4096:
        data.extend(await client.upload(min(chunk_size, 4096 - len(data))))

    # Print all data
    for offset in range(0, len(data), 16):
        chunk = data[offset : offset + 16]
        hex_data = chunk.hex(" ")
        ascii_data = "".join(chr(byte) if 32 <= byte < 127 else "." for byte in chunk)
        address = 0x8000_0000 + offset // info.address_granularity
        print(f"{address:08x}  {hex_data:<47}  |{ascii_data}|")


if __name__ == "__main__":
    asyncio.run(main())
