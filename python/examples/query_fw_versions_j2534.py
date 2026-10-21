"""Query ECU firmware versions through native J2534, reusing the device handle.

Requires Windows and a compatible vendor PassThru DLL.
The device handle is reused sequentially when switching ECU channels.
"""

import asyncio

from automotive.adapters import J2534Config, available_adapters
from automotive.adapters.j2534 import J2534Device, J2534NativeIsoTpTransport
from automotive.can.bitrate import BitrateSettings
from automotive.errors import AutomotiveError
from automotive.protocols.uds import DataIdentifier, UDSClient
from automotive.transports.isotp import IsoTPConfig


async def get_version(transport: J2534NativeIsoTpTransport, identifier: int) -> None:
    """Read named DIDs from one ECU through its current native channel."""
    uds = UDSClient(transport)
    await uds.tester_present()
    for did in DataIdentifier:
        try:
            response = await uds.read_data_by_identifier(did)
        except (AutomotiveError, OSError):
            continue
        print(f"{identifier:x} 0x{did.value:x} {did.name}: {response!r}")


async def main() -> None:
    """Switch native channels, closing the final device even after errors."""
    if "j2534" not in available_adapters():
        print("This example requires Windows and a compatible PassThru DLL.")
        return
    adapter_config = J2534Config(bitrate=BitrateSettings(bitrate=500_000))
    identifiers = [*range(0x700, 0x7F8), *(0x18DA0000 + (i << 8) + 0xF1 for i in range(256))]
    device: J2534Device | None = None
    try:
        for identifier in identifiers:
            config = IsoTPConfig(tx_id=identifier)
            if device is None:
                transport = await J2534NativeIsoTpTransport.open(config, adapter_config)
            else:
                try:
                    transport = await J2534NativeIsoTpTransport.new_on_device(device, config, adapter_config)
                except AutomotiveError:
                    transport = await J2534NativeIsoTpTransport.open(config, adapter_config)
            device = None
            async with transport:
                try:
                    await get_version(transport, identifier)
                except (AutomotiveError, OSError):
                    pass
                device = await transport.into_device()
    finally:
        if device is not None:
            await device.aclose()


if __name__ == "__main__":
    asyncio.run(main())
