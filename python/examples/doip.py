"""Trigger vehicle identification and read VIN using UDS over DoIP."""

import asyncio

from automotive.protocols.uds import DataIdentifier, UDSClient
from automotive.transports.doip import discover


async def main() -> None:
    """Discover vehicles with defaults, then connect using their advertised addresses."""
    for vehicle in await discover():
        print(vehicle.address, vehicle.vin)
        async with await vehicle.connect() as transport:
            print(await UDSClient(transport).read_data_by_identifier(DataIdentifier.VIN))


if __name__ == "__main__":
    asyncio.run(main())
