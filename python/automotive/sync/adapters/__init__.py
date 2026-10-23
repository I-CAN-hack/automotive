"""Blocking CAN adapter classes using the shared configuration API."""

from __future__ import annotations

from typing import Self

from ... import adapters as _adapters
from ...adapters import (
    AdapterConfig,
    J2534Config,
    PandaConfig,
    PeakConfig,
    SocketCanConfig,
    VectorConfig,
    _adapter_config,
    available_adapters,
)
from .._base import Receiver, Resource
from .._runtime import call, run
from ..can import Frame


class CanAdapter(Resource[_adapters.CanAdapter]):
    """Open a CAN adapter immediately, using discovery when no config is supplied.

    Args:
        config: Shared backend configuration; None discovers an adapter with
            Rust defaults. Named classes such as PeakAdapter select a backend.

    Hardware is released when its last owner is dropped. Transports, clients,
    and receivers retain ownership. close() and with support deterministic
    cleanup. Explicit closure invalidates dependent operations, and a closed
    adapter cannot be reopened. Opening errors are raised by construction.
    """

    def __init__(self, config: AdapterConfig | None = None) -> None:
        super().__init__(_adapters.CanAdapter(config))

    @property
    def config(self) -> AdapterConfig | None:
        """Backend configuration, or None for automatic discovery."""
        return self._async.config

    def open(self) -> Self:
        """Return this already-open adapter; raise RuntimeError after closure."""
        run(self._async.open)
        return self

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
            Subscribe before sending. Dropping the receiver releases its
            subscription; close() releases it early.
        """
        return Receiver(call(self._async.recv))

    def recv_for(self, duration: float) -> list[Frame]:
        """Collect CAN frames, including transmit echoes, for duration seconds.

        Returns frames in receive order, or finishes early if the adapter closes.
        Uses an independent subscription that closes on return; existing recv()
        subscriptions are unaffected. duration must be finite and nonnegative.
        """
        return run(self._async.recv_for, duration)


class SocketCanAdapter(CanAdapter):
    """Open a Linux SocketCAN adapter with ``SocketCanAdapter('can0')``.

    Args:
        interface: Interface name, default "can0", or a reusable SocketCanConfig.

    Timing is configured through the OS. Construction opens synchronously.
    """

    def __init__(self, interface: str | SocketCanConfig = "can0") -> None:
        config = SocketCanConfig(interface=interface) if isinstance(interface, str) else interface
        super().__init__(_adapter_config(config, SocketCanConfig))


class PandaAdapter(CanAdapter):
    """Open a panda USB adapter immediately.

    Args:
        config: Reusable PandaConfig; None uses Rust defaults.
    """

    def __init__(self, config: PandaConfig | None = None) -> None:
        super().__init__(_adapter_config(config, PandaConfig))


class PeakAdapter(CanAdapter):
    """Open a PEAK PCAN-USB FD adapter immediately.

    Args:
        config: Reusable PeakConfig; None uses Rust defaults.
    """

    def __init__(self, config: PeakConfig | None = None) -> None:
        super().__init__(_adapter_config(config, PeakConfig))


class VectorAdapter(CanAdapter):
    """Open a Windows Vector XL adapter immediately.

    Args:
        config: Reusable VectorConfig; None retains shared channel access.
    """

    def __init__(self, config: VectorConfig | None = None) -> None:
        super().__init__(_adapter_config(config, VectorConfig))


class J2534Adapter(CanAdapter):
    """Open a Windows J2534 raw CAN adapter immediately.

    Args:
        config: Reusable J2534Config; None discovers a DLL and uses Rust defaults.

    Use J2534NativeIsoTP for firmware-managed ISO-TP.
    """

    def __init__(self, config: J2534Config | None = None) -> None:
        super().__init__(_adapter_config(config, J2534Config))


__all__ = [
    "AdapterConfig",
    "CanAdapter",
    "J2534Adapter",
    "J2534Config",
    "PandaAdapter",
    "PandaConfig",
    "PeakAdapter",
    "PeakConfig",
    "SocketCanAdapter",
    "SocketCanConfig",
    "VectorAdapter",
    "VectorConfig",
    "available_adapters",
]
