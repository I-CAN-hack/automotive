"""Read ECU VINs using active or passive DoIP discovery over IPv4 or IPv6."""

import argparse
import asyncio
import ipaddress
import socket
from collections.abc import AsyncIterator

from automotive.protocols.uds import DataIdentifier, UDSClient, UDSConfig
from automotive.transports.doip import Discovery, DiscoveryConfig, DoIPConfig, VehicleAnnouncement, discover


def parse_address(value: str) -> int:
    return int(value, 0)


async def announcements(config: DiscoveryConfig, mode: str) -> AsyncIterator[VehicleAnnouncement]:
    if mode == "active":
        for vehicle in await discover(config):
            yield vehicle
    else:
        async with await Discovery.listen(config) as listener:
            print("Listening for announcements; power up the ECU. Press Ctrl+C to stop.", flush=True)
            while True:
                yield await listener.recv()


async def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--discovery", choices=("active", "passive"), default="active")
    parser.add_argument("--adapter", help="IPv6 interface (required with --ipv6)")
    parser.add_argument("--local-address", help="Local IPv4 address/prefix (required for active IPv4 discovery)")
    family = parser.add_mutually_exclusive_group()
    family.add_argument("--ipv4", action="store_true", help="Use IPv4 broadcast (default)")
    family.add_argument("--ipv6", action="store_true", help="Use IPv6 multicast")
    parser.add_argument("--source-address", type=parse_address, default=0x0E89)
    args = parser.parse_args()
    if args.ipv6 and not args.adapter:
        parser.error("--ipv6 requires --adapter")
    if not args.ipv6 and args.discovery == "active" and not args.local_address:
        parser.error("active IPv4 discovery requires --local-address")

    local_address = None
    if args.ipv6:
        config = DiscoveryConfig.ipv6(socket.if_nametoindex(args.adapter))
    elif args.discovery == "active":
        address = ipaddress.IPv4Interface(args.local_address)
        local_address = f"{address.ip}:0"
        config = DiscoveryConfig(
            bind_address=local_address,
            destination=f"{address.network.broadcast_address}:13400",
        )
    else:
        config = DiscoveryConfig()

    async for vehicle in announcements(config, args.discovery):
        doip_config = DoIPConfig(
            source_address=args.source_address,
            protocol_version=vehicle.protocol_version,
            local_address=local_address,
        )
        async with await vehicle.connect(doip_config) as transport:
            uds = UDSClient(transport, UDSConfig(p2_timeout=1.0))
            vin = await uds.read_data_by_identifier(DataIdentifier.VIN)
            print(vehicle.address, f"0x{vehicle.logical_address:04x}", "VIN:", vin.decode("ascii"), flush=True)


if __name__ == "__main__":
    asyncio.run(main())
