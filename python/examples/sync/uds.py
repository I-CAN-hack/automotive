"""Read software identification from an ECU without asyncio."""

from automotive.sync.adapters import Adapter
from automotive.sync.protocols.uds import DataIdentifier, SessionType, UDSClient
from automotive.sync.transports.isotp import IsoTPAdapter


def main() -> None:
    """Use defaults, select a session, and read an ECU data identifier."""
    adapter = Adapter()
    transport = IsoTPAdapter.from_id(adapter, 0x7A1)
    uds = UDSClient(transport)
    uds.tester_present()
    uds.diagnostic_session_control(SessionType.EXTENDED_DIAGNOSTIC)
    did = DataIdentifier.APPLICATION_SOFTWARE_IDENTIFICATION
    print(did.name, repr(uds.read_data_by_identifier(did)))


if __name__ == "__main__":
    main()
