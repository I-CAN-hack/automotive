"""Windows J2534 native ISO-TP and reusable PassThru device handles."""

from __future__ import annotations

from typing import Self

from .. import _native
from .._base import AsyncResource
from ..transports.isotp import IsoTPAdapter, IsoTPConfig
from . import Adapter, J2534Config


class J2534Device(AsyncResource[_native._Device]):
    """A reusable device returned by a native transport's into_device method.

    Opening a new channel consumes this handle. Dropping its last owner releases
    the device; aclose() supports early cleanup. Creating multiple channels
    concurrently is unsupported.
    """

    def __init__(self, native: _native._Device) -> None:
        super().__init__(native)


class J2534NativeIsoTpTransport(IsoTPAdapter):
    """Firmware-managed ISO-TP transport; use the asynchronous open factory.

    The vendor DLL must support concurrent read/write calls. Native J2534 uses
    Rust's existing ISO-TP configuration subset; bus, fd, and max_dlen do not
    configure a native ISO15765 channel. Other fields follow Rust behavior.
    """

    adapter_config: J2534Config = J2534Config()

    def __new__(cls, adapter: Adapter, config: IsoTPConfig) -> Self:
        raise TypeError("use await J2534NativeIsoTpTransport.open(config)")

    @classmethod
    async def open(cls, config: IsoTPConfig, adapter_config: J2534Config | None = None) -> Self:
        """Open a device and native channel without blocking the Python loop.

        Args:
            config: ISO-TP addressing, padding, timing, and flow-control settings.
            adapter_config: DLL selection and bitrate; None discovers the DLL
                and uses 500 kbit/s Classic CAN.

        Returns:
            An open transport supporting async with and UDSClient.

        Raises:
            NotSupportedError: Native J2534 is unavailable on this platform/build.
            AdapterError: The vendor device or channel could not be opened.
        """
        result = object.__new__(cls)
        result.config = config
        result.adapter_config = adapter_config or J2534Config()
        result._native = await _native.open_native_isotp(result.adapter_config, config)
        return result

    @classmethod
    async def new_on_device(
        cls, device: J2534Device, config: IsoTPConfig, adapter_config: J2534Config | None = None
    ) -> Self:
        """Consume a reusable device to open the next native ISO-TP channel.

        The handle is consumed even if channel opening fails, matching Rust.
        No requests may be issued on the previous channel after transfer.
        """
        result = object.__new__(cls)
        result.config = config
        result.adapter_config = adapter_config or J2534Config()
        result._native = await _native.open_native_isotp(result.adapter_config, config, device._native)
        return result

    async def into_device(self) -> J2534Device:
        """Close this channel and retain its device for reuse.

        Returns:
            A device handle consumable by new_on_device. Existing clients and
            receivers are invalidated. Close the returned handle after use.
        """
        return J2534Device(await self._native.into_device())


__all__ = ["J2534Config", "J2534Device", "J2534NativeIsoTpTransport"]
