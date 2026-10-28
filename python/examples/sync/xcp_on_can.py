"""Read 4 KiB using SET_MTA and UPLOAD over XCP on CAN and CAN FD."""

import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))


from automotive.sync.adapters import CanAdapter
from automotive.sync.can import Frame
from automotive.sync.protocols.xcp import XCPCanConfig, XCPClient, XCPOnCAN
from examples.utils import hexdump


def main() -> None:
    adapter = CanAdapter()

    # Wake ECU
    wake = Frame(id=0x541, data=b"\xff" * 8)
    for _ in range(10):
        adapter.send(wake)
        time.sleep(0.1)

    config = XCPCanConfig(tx_id=0x6C3, rx_id=0x6C4, fd=False)
    client = XCPClient(XCPOnCAN(adapter, config))
    info = client.connect()
    print(f"Connected: {info}")
    chunk_size = info.max_upload_size

    # Read using UPLOAD
    client.set_mta(0x8000_0000)
    data = bytearray()
    while len(data) < 4096:
        data.extend(client.upload(min(chunk_size, 4096 - len(data))))

    # Print all data
    print(hexdump(data, 0x8000_0000, address_granularity=info.address_granularity))


if __name__ == "__main__":
    main()
