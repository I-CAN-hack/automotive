"""Exchange short and segmented PDUs with an ECU at 0x7A1/0x7A9."""

import asyncio

from automotive.adapters import CanAdapter
from automotive.transports.isotp import IsoTP, IsoTPConfig


async def main() -> None:
    """Send short and segmented requests, printing each response as hex."""
    adapter = CanAdapter()
    config = IsoTPConfig(tx_id=0x7A1)
    isotp = IsoTP(adapter, config)

    stream = isotp.recv()

    await isotp.send(b"\x3e\x00")
    response = await anext(stream)
    print("RX:", response.hex())

    await isotp.send(b"\x22\xf1\x81")
    response = await anext(stream)
    print("RX:", response.hex())

    long_request = b"\x10" + b"\x00" * 32
    await isotp.send(long_request)
    response = await anext(stream)
    print("RX:", response.hex())

    long_response = b"\x22\xf1\x81"
    await isotp.send(long_response)
    response = await anext(stream)
    print("RX:", response.hex())


if __name__ == "__main__":
    asyncio.run(main())
