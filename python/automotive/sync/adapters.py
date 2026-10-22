"""Blocking CAN adapter classes using the shared configuration API."""

from __future__ import annotations

from typing import Self

from .. import adapters as _adapters
from ..adapters import AdapterConfig, J2534Config, PandaConfig, PeakConfig, SocketCanConfig, VectorConfig
from ..can import Frame
from ._runtime import Receiver, Resource, call, run


class Adapter(Resource[_adapters.Adapter]):
    """Discover a CAN adapter with Rust defaults when opened.

    Construction performs no I/O. Prefer ``with Adapter() as adapter`` or call
    open() explicitly. close() releases hardware and invalidates dependent
    transports and receivers. A closed adapter cannot be reopened.
    """

    def __init__(self) -> None:
        super().__init__(_adapters.Adapter())

    @property
    def config(self) -> AdapterConfig | None:
        """Backend configuration, or None for automatic discovery."""
        return self._async.config

    def open(self) -> Self:
        """Open this adapter and return self; repeated calls retain the connection.

        Raises:
            ValueError: Explicit configuration is invalid.
            NotSupportedError: The backend is unavailable on this platform.
            AdapterError: Opening failed or discovery found no usable adapter.
            RuntimeError: This adapter has already been closed.
        """
        run(self._async.open)
        return self

    def __enter__(self) -> Self:
        """Open this adapter and return it."""
        self.open()
        return super().__enter__()

    def send(self, frame: Frame) -> None:
        """Send a frame and block for the backend's transmit confirmation.

        Args:
            frame: A Classic, FD, or remote frame supported by the adapter.

        Raises:
            TimeoutError: A configured transmit deadline expired.
            TransmissionCancelledError: A queue clear invalidated this send.
            NotSupportedError: The backend cannot encode this frame.
            AdapterError: The hardware operation failed.

        Confirmation strength depends on the backend. Interrupting this call
        cannot undo a frame that has already been sent.
        """
        run(self._async.send, frame)

    def recv(self) -> Receiver[Frame]:
        """Subscribe immediately to CAN frames, including transmit echoes.

        Returns:
            A blocking iterator of Frame objects supporting with and close().
            Subscribe before sending, then close the receiver after use.
        """
        return Receiver(call(self._async.recv))


class SocketCanAdapter(Adapter):
    """A Linux SocketCAN adapter; use ``with SocketCanAdapter('can0')``.

    Args:
        interface: Interface name, default "can0", or a reusable SocketCanConfig.

    Timing is configured through the OS. Construction performs no I/O.
    """

    def __init__(self, interface: str | SocketCanConfig = "can0") -> None:
        super().__init__()
        self._async = _adapters.SocketCanAdapter(interface)


class PandaAdapter(Adapter):
    """A panda USB adapter; use a with block or call open().

    Args:
        config: Reusable PandaConfig; None uses Rust defaults.
    """

    def __init__(self, config: PandaConfig | None = None) -> None:
        super().__init__()
        self._async = _adapters.PandaAdapter(config)


class PeakAdapter(Adapter):
    """A PEAK PCAN-USB FD adapter; use a with block or call open().

    Args:
        config: Reusable PeakConfig; None uses Rust defaults.
    """

    def __init__(self, config: PeakConfig | None = None) -> None:
        super().__init__()
        self._async = _adapters.PeakAdapter(config)


class VectorAdapter(Adapter):
    """A Windows Vector XL adapter; use a with block or call open().

    Args:
        config: Reusable VectorConfig; None retains shared channel access.
    """

    def __init__(self, config: VectorConfig | None = None) -> None:
        super().__init__()
        self._async = _adapters.VectorAdapter(config)


class J2534Adapter(Adapter):
    """A Windows J2534 raw CAN adapter; use a with block or call open().

    Args:
        config: Reusable J2534Config; None discovers a DLL and uses Rust defaults.

    Use J2534NativeIsoTpTransport for firmware-managed ISO-TP.
    """

    def __init__(self, config: J2534Config | None = None) -> None:
        super().__init__()
        self._async = _adapters.J2534Adapter(config)


__all__ = ["Adapter", "J2534Adapter", "PandaAdapter", "PeakAdapter", "SocketCanAdapter", "VectorAdapter"]
