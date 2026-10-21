"""Query named DIDs across standard and extended ECU addresses concurrently."""

import asyncio

from automotive.adapters import Adapter
from automotive.errors import AutomotiveError
from automotive.protocols.uds import DataIdentifier, UDSClient
from automotive.transports.isotp import IsoTPAdapter, IsoTPConfig


async def get_version(adapter: Adapter, identifier: int) -> None:
    """Query each named DID for one ECU, retaining its configured transport."""
    async with IsoTPAdapter(adapter, IsoTPConfig(tx_id=identifier, timeout=1.0)) as transport:
        uds = UDSClient(transport)
        await uds.tester_present()
        for did in DataIdentifier:
            try:
                response = await uds.read_data_by_identifier(did)
            except (AutomotiveError, OSError):
                continue
            print(f"{identifier:x} 0x{did.value:x} {did.name}: {response!r}")


async def main() -> None:
    """Scan the same standard/extended address ranges as the Rust example."""
    identifiers = [*range(0x700, 0x7F8), *(0x18DA0000 + (i << 8) + 0xF1 for i in range(256))]
    async with Adapter() as adapter:
        await asyncio.gather(*(get_version(adapter, identifier) for identifier in identifiers), return_exceptions=True)


if __name__ == "__main__":
    asyncio.run(main())
