"""Exchange short and segmented PDUs with an ECU at 0x7A1/0x7A9."""

from automotive.sync.adapters import CanAdapter
from automotive.sync.transports.isotp import IsoTP, IsoTPConfig


def main() -> None:
    """Send short and segmented requests, printing each response as hex."""
    adapter = CanAdapter()
    config = IsoTPConfig(tx_id=0x7A1)
    isotp = IsoTP(adapter, config)

    stream = isotp.recv()

    isotp.send(b"\x3e\x00")
    response = next(stream)
    print("RX:", response.hex())

    isotp.send(b"\x22\xf1\x81")
    response = next(stream)
    print("RX:", response.hex())

    long_request = b"\x10" + b"\x00" * 32
    isotp.send(long_request)
    response = next(stream)
    print("RX:", response.hex())

    long_response = b"\x22\xf1\x81"
    isotp.send(long_response)
    response = next(stream)
    print("RX:", response.hex())


if __name__ == "__main__":
    main()
