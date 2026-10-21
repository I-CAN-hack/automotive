"""Read software identification from a diagnostic ECU at transmit ID 0x7A1."""

import asyncio

from automotive.adapters import get_adapter
from automotive.protocols.uds import DataIdentifier, SessionType, UDSClient
from automotive.transports.isotp import IsoTPAdapter


async def main() -> None:
    """Use defaults, select a session, and read an ECU data identifier."""
    async with await get_adapter() as adapter, IsoTPAdapter.from_id(adapter, 0x7A1) as transport:
        uds = UDSClient(transport)
        await uds.tester_present()
        await uds.diagnostic_session_control(SessionType.EXTENDED_DIAGNOSTIC)
        did = DataIdentifier.APPLICATION_SOFTWARE_IDENTIFICATION
        print(did.name, repr(await uds.read_data_by_identifier(did)))


if __name__ == "__main__":
    asyncio.run(main())
