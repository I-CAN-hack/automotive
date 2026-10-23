"""Blocking Windows J2534 native ISO-TP and reusable PassThru device handles."""

from __future__ import annotations

from typing import Self

from ...adapters import j2534 as _j2534
from .._base import Resource
from .._runtime import run
from ..transports import Transport
from ..transports.isotp import IsoTPConfig
from . import J2534Config


class J2534Device(Resource[_j2534.J2534Device]):
    """A reusable J2534 device obtained from into_device().

    Dropping the last owner releases the device; with and close() support early
    cleanup. Opening a new native channel
    consumes this handle, even if opening fails. Concurrent channels on the
    same device are unsupported.
    """


class J2534NativeIsoTP(Transport):
    """Firmware-managed Windows ISO-TP; create it with open().

    The vendor DLL must support concurrent reads and writes. Configuration
    follows the async native transport, including its ISO15765 limitations.
    """

    _async: _j2534.J2534NativeIsoTP

    def __new__(cls, config: IsoTPConfig, adapter_config: J2534Config | None = None) -> Self:
        raise TypeError("use J2534NativeIsoTP.open(config)")

    @property
    def config(self) -> IsoTPConfig:
        """The shared ISO-TP configuration used by this native channel."""
        return self._async.config

    @property
    def adapter_config(self) -> J2534Config:
        """DLL selection and bitrate used to open this native channel."""
        return self._async.adapter_config

    @classmethod
    def open(cls, config: IsoTPConfig, adapter_config: J2534Config | None = None) -> Self:
        """Open a J2534 device and native ISO-TP channel, blocking until ready.

        Args:
            config: Shared ISO-TP addressing and flow-control settings.
            adapter_config: DLL selection and bitrate; None uses Rust defaults.

        Returns:
            An open transport supporting ordinary with and synchronous UDSClient.

        Raises:
            NotSupportedError: Native J2534 is unavailable on this platform.
            AdapterError: The vendor device or channel could not be opened.
        """
        result = object.__new__(cls)
        result._async = run(_j2534.J2534NativeIsoTP.open, config, adapter_config)
        return result

    @classmethod
    def new_on_device(cls, device: J2534Device, config: IsoTPConfig, adapter_config: J2534Config | None = None) -> Self:
        """Consume a reusable device to open the next native ISO-TP channel.

        The previous channel's clients and receivers remain invalidated. The
        device handle is consumed even if opening fails, matching Rust.
        """
        result = object.__new__(cls)
        result._async = run(_j2534.J2534NativeIsoTP.new_on_device, device._async, config, adapter_config)
        return result

    def into_device(self) -> J2534Device:
        """Close this channel and return its device for reuse or explicit closure.

        Existing clients and receivers are invalidated. Pass the returned
        device to new_on_device() to open another native channel.
        """
        return J2534Device(run(self._async.into_device))


__all__ = ["J2534Config", "J2534Device", "J2534NativeIsoTP"]
