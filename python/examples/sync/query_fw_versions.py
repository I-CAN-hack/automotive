"""Query named DIDs across standard and extended ECU addresses."""

from automotive.sync.adapters import CanAdapter
from automotive.sync.errors import AutomotiveError
from automotive.sync.protocols.uds import DataIdentifier, UDSClient, UDSConfig
from automotive.sync.transports.isotp import IsoTP, IsoTPConfig


def get_version(adapter: CanAdapter, identifier: int) -> None:
    """Query each named DID for one ECU, retaining its configured transport."""
    # Lower timeout for the sync version since we scan each ECU individually and bus load is low
    config = IsoTPConfig(tx_id=identifier, timeout=0.05)
    isotp = IsoTP(adapter, config)
    uds = UDSClient(isotp, UDSConfig(p2_timeout=0.05))

    uds.tester_present()

    for did in DataIdentifier:
        try:
            resp = uds.read_data_by_identifier(did)
        except (AutomotiveError, OSError):
            continue
        print(f"{identifier:x} 0x{did.value:x} {did.name}: {resp!r}")


def main() -> None:
    """Scan the Rust example's standard/extended address ranges sequentially."""
    adapter = CanAdapter()

    standard_ids = range(0x700, 0x7F8)
    extended_ids = (0x18DA0000 + (i << 8) + 0xF1 for i in range(0x100))
    ids = [*standard_ids, *extended_ids]

    for identifier in ids:
        try:
            get_version(adapter, identifier)
        except (AutomotiveError, OSError):
            continue


if __name__ == "__main__":
    main()
