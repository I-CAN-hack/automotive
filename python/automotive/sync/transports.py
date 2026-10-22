"""Blocking ISO-TP transports, including native Windows J2534 channels."""

from __future__ import annotations

from typing import Self

from .._base import Buffer
from ..adapters import J2534Config
from ..adapters import j2534 as _j2534
from ..can import Identifier
from ..transports import isotp as _isotp
from ..transports.isotp import IsoTPConfig
from ._runtime import Receiver, Resource, call, run
from .adapters import Adapter


class IsoTPAdapter(Resource[_isotp.IsoTPAdapter]):
    """A blocking complete-message ISO-TP transport retaining its CAN adapter.

    Args:
        adapter: Open synchronous CAN adapter.
        config: Shared ISO-TP configuration; only tx_id is required.

    Closing this transport leaves the CAN adapter open. Closing the adapter
    invalidates this transport and its clients. Close raw ISO-TP receivers
    before issuing UDS requests on the same transport.
    """

    def __init__(self, adapter: Adapter, config: IsoTPConfig) -> None:
        super().__init__(call(_isotp.IsoTPAdapter, adapter._async, config))

    @property
    def config(self) -> IsoTPConfig:
        """The shared ISO-TP configuration used by this transport."""
        return self._async.config

    @classmethod
    def from_id(cls, adapter: Adapter, tx_id: int | Identifier) -> Self:
        """Create a transport with defaults and a Rust-derived receive ID.

        Args:
            adapter: Open synchronous CAN adapter.
            tx_id: Integer or explicit transmit identifier.
        """
        return cls(adapter, IsoTPConfig(tx_id=tx_id))

    def send(self, data: Buffer) -> None:
        """Transmit one nonempty PDU, blocking until Rust completes the transfer.

        Args:
            data: Bytes, bytearray, or a contiguous byte-oriented memoryview.

        Raises:
            ValueError: The payload is empty or exceeds the protocol limit.
            TimeoutError: Confirmation or required flow control timed out.
            IsoTPError: The peer rejected the transfer or framing failed.
        """
        run(self._async.send, data)

    def recv(self) -> Receiver[bytes]:
        """Subscribe immediately to complete PDUs as bytes.

        Returns:
            A blocking iterator supporting with and close(). Subscribe before
            sending. A receive timeout raises TimeoutError; next() can resume.
        """
        return Receiver(call(self._async.recv))


class J2534Device(Resource[_j2534.J2534Device]):
    """A reusable J2534 device obtained from into_device().

    Use with or close() to release the device. Opening a new native channel
    consumes this handle, even if opening fails. Concurrent channels on the
    same device are unsupported.
    """


class J2534NativeIsoTpTransport(IsoTPAdapter):
    """Firmware-managed Windows ISO-TP; create it with open().

    The vendor DLL must support concurrent reads and writes. Configuration
    follows the async native transport, including its ISO15765 limitations.
    """

    _async: _j2534.J2534NativeIsoTpTransport

    def __new__(cls, adapter: Adapter, config: IsoTPConfig) -> Self:
        raise TypeError("use J2534NativeIsoTpTransport.open(config)")

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
        result._async = run(_j2534.J2534NativeIsoTpTransport.open, config, adapter_config)
        return result

    @classmethod
    def new_on_device(cls, device: J2534Device, config: IsoTPConfig, adapter_config: J2534Config | None = None) -> Self:
        """Consume a reusable device to open the next native ISO-TP channel.

        The previous channel's clients and receivers remain invalidated. The
        device handle is consumed even if opening fails, matching Rust.
        """
        result = object.__new__(cls)
        result._async = run(_j2534.J2534NativeIsoTpTransport.new_on_device, device._async, config, adapter_config)
        return result

    def into_device(self) -> J2534Device:
        """Close this channel and return its device for reuse or explicit closure.

        Existing clients and receivers are invalidated. Pass the returned
        device to new_on_device() to open another native channel.
        """
        return J2534Device(run(self._async.into_device))


__all__ = ["IsoTPAdapter", "J2534Device", "J2534NativeIsoTpTransport"]
