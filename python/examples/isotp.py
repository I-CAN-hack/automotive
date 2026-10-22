"""Exchange short and segmented PDUs with an ECU at 0x7A1/0x7A9."""

import asyncio

from automotive.adapters import Adapter
from automotive.transports.isotp import IsoTPAdapter, IsoTPConfig


async def main() -> None:
    """Subscribe before sending so fast ECU replies are retained."""
    adapter = Adapter()
    transport = IsoTPAdapter(adapter, IsoTPConfig(tx_id=0x7A1))
    messages = transport.recv()
    for request in (b"\x3e\x00", b"\x22\xf1\x81", b"\x10" + bytes(31), b"\x22\xf1\x81"):
        await transport.send(request)
        print("RX:", (await anext(messages)).hex())


if __name__ == "__main__":
    asyncio.run(main())
