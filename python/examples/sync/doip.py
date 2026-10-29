"""Trigger vehicle identification and read VIN using blocking UDS over DoIP."""

from automotive.sync.protocols.uds import DataIdentifier, UDSClient
from automotive.sync.transports.doip import discover


def main() -> None:
    """Discover vehicles with defaults, then connect using their advertised addresses."""
    for vehicle in discover():
        print(vehicle.address, vehicle.vin)
        with vehicle.connect() as transport:
            print(UDSClient(transport).read_data_by_identifier(DataIdentifier.VIN))


if __name__ == "__main__":
    main()
