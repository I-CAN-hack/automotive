"""Dump 4 KiB of ECU flash using CCP SHORT_UP."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from automotive.sync.adapters import CanAdapter
from automotive.sync.protocols.ccp import CCPCanConfig, CCPClient, CCPConfig, CCPOnCAN
from examples.utils import hexdump


def main() -> None:
    can = CCPCanConfig(tx_id=0x6EC, rx_id=0x6ED)

    config = CCPConfig(byte_order="little")
    adapter = CanAdapter()

    # Connect to CCP
    client = CCPClient(CCPOnCAN(adapter, can), config)
    client.connect(station_address=0)

    # Read using SHORT_UP
    start = 0x8000_0000
    data = bytearray()
    while len(data) < 0x1000:
        data.extend(client.short_upload(start + len(data), 4))

    # Print all data
    print(hexdump(data, start))


if __name__ == "__main__":
    main()
