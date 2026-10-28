"""Read 4 KiB using SET_MTA and UPLOAD over XCP on Ethernet."""

import argparse

from automotive.sync.protocols.xcp import XCPClient, XCPConfig, XCPOnUDP


def parse_address(value: str) -> int:
    address = int(value, 0)
    if not 0 <= address <= 0xFFFF_FFFF:
        raise argparse.ArgumentTypeError("address must fit in 32 bits")
    return address


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, help="ECU host")
    parser.add_argument("--port", required=True, type=int, help="ECU UDP port")
    parser.add_argument("--bind", required=True, help="Local host to bind")
    parser.add_argument("--bind-port", required=True, type=int, help="Local UDP port to bind")
    parser.add_argument("--address", required=True, type=parse_address, help="Start address in hexadecimal or decimal")
    args = parser.parse_args()

    transport = XCPOnUDP.connect(args.host, args.port, timeout=2.0, local_addr=(args.bind, args.bind_port))
    client = XCPClient(transport, XCPConfig(timeout=2.0))

    info = client.connect()
    print(f"Connected: {info}")
    chunk_size = info.max_upload_size
    print(f"Reading 4096 bytes at {args.address:#010x}, up to {chunk_size} bytes per request")

    # Read using UPLOAD
    client.set_mta(args.address)
    data = bytearray()
    while len(data) < 4096:
        data.extend(client.upload(min(chunk_size, 4096 - len(data))))

    # Print all data
    for offset in range(0, len(data), 16):
        chunk = data[offset : offset + 16]
        hex_data = chunk.hex(" ")
        ascii_data = "".join(chr(byte) if 32 <= byte < 127 else "." for byte in chunk)
        address = args.address + offset // info.address_granularity
        print(f"{address:08x}  {hex_data:<47}  |{ascii_data}|")


if __name__ == "__main__":
    main()
