"""Read software identification from a diagnostic ECU at transmit ID 0x7A1."""

import asyncio

from automotive.adapters import CanAdapter
from automotive.protocols.uds import DataIdentifier, SessionType, UDSClient
from automotive.transports.isotp import IsoTP


async def main() -> None:
    """Use defaults, select a session, and read an ECU data identifier."""
    adapter = CanAdapter()
    isotp = IsoTP.from_id(adapter, 0x7A1)
    uds = UDSClient(isotp)
    await uds.tester_present()
    await uds.diagnostic_session_control(SessionType.EXTENDED_DIAGNOSTIC)

    did = DataIdentifier.APPLICATION_SOFTWARE_IDENTIFICATION
    resp = await uds.read_data_by_identifier(did)

    print(f"{did.name}: {resp!r}")


if __name__ == "__main__":
    asyncio.run(main())
