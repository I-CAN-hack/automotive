"""Query named DIDs across standard and extended ECU addresses."""

import asyncio

from automotive.adapters import Adapter
from automotive.errors import AutomotiveError
from automotive.protocols.uds import DataIdentifier, UDSClient, UDSConfig
from automotive.transports.isotp import IsoTPAdapter, IsoTPConfig


async def get_version(adapter: Adapter, identifier: int) -> None:
    """Query each named DID for one ECU, retaining its configured transport."""
    # Increased timeout for adapters not supporting real ACKs.
    config = IsoTPConfig(tx_id=identifier, timeout=1.0)
    isotp = IsoTPAdapter(adapter, config)
    uds = UDSClient(isotp, UDSConfig(p2_timeout=1.0))

    await uds.tester_present()

    for did in DataIdentifier:
        try:
            resp = await uds.read_data_by_identifier(did)
        except (AutomotiveError, OSError):
            continue
        print(f"{identifier:x} 0x{did.value:x} {did.name}: {resp!r}")


async def main() -> None:
    """Scan the Rust example's standard/extended address ranges concurrently."""
    adapter = Adapter()

    standard_ids = range(0x700, 0x7F8)
    extended_ids = (0x18DA0000 + (i << 8) + 0xF1 for i in range(0x100))
    ids = [*standard_ids, *extended_ids]

    await asyncio.gather(*(get_version(adapter, identifier) for identifier in ids), return_exceptions=True)


if __name__ == "__main__":
    asyncio.run(main())
