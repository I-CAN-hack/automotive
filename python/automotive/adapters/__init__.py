"""Hardware discovery, explicit adapter selection, and reusable configuration.

Use ``CanAdapter()`` for discovery, or select a backend with ``PeakAdapter(config)``.
Constructors open synchronously, releasing the GIL while Rust accesses hardware.
Sending, receiving, and explicit closure are asynchronous. SocketCanAdapter also
accepts an interface name directly. Context managers remain optional.
SocketCAN uses OS-configured timing; USB adapters require their permissions and
Windows adapters require the vendor driver/DLL.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import ClassVar, Self, TypeAlias, TypeVar

from .. import _native as _rust
from .._base import Receiver
from ..can import Frame
from ..can.bitrate import BitrateConfig, BitrateSettings, _resolved

_DEFAULTS = _rust.defaults()


@dataclass(frozen=True, slots=True, kw_only=True)
class _TimingConfig:
    backend: ClassVar[str]
    bitrate: BitrateSettings | None = None

    @property
    def resolved_bitrate(self) -> BitrateConfig | None:
        """Calculated timing for this backend, including its defaults."""
        return _resolved(self.backend, self.bitrate)


@dataclass(frozen=True, slots=True, kw_only=True)
class SocketCanConfig:
    """Select an existing Linux SocketCAN interface.

    Args:
        interface: Interface name, default "can0". Use "vcan0" for tests.

    Bus timing is configured through the OS; opening never changes it.
    """

    interface: str = "can0"
    backend: ClassVar[str] = "socketcan"


@dataclass(frozen=True, slots=True, kw_only=True)
class PandaConfig(_TimingConfig):
    """Configure a panda USB adapter.

    Args:
        bitrate: Reusable timing inputs. None uses Rust's 500 kbit/s nominal
            and 2 Mbit/s FD data timing, with 80% sample points.

    An explicit BitrateSettings starts without presets. A nominal bitrate
    alone selects Classic CAN. USB access permissions are required.
    """

    backend: ClassVar[str] = "panda"


@dataclass(frozen=True, slots=True, kw_only=True)
class PeakConfig(_TimingConfig):
    """Configure a PEAK PCAN-USB FD adapter.

    Args:
        bitrate: Timing inputs. None uses Rust's 500 kbit/s nominal and
            2 Mbit/s data timing, with 80% sample points.
        tx_timeout: Queue-admission/confirmation deadline in seconds; default
            one second. None disables the deadline; zero is invalid.

    A timeout clears the transmit queue, cancelling other affected sends.
    BRS requires a configured FD data bitrate.
    """

    tx_timeout: float | None = _DEFAULTS["tx_timeout"]
    backend: ClassVar[str] = "peak"


@dataclass(frozen=True, slots=True, kw_only=True)
class VectorConfig(_TimingConfig):
    """Configure a Windows Vector XL channel.

    Args:
        channel: Global channel index from the XL driver, default zero.
        bitrate: None retains existing timing with shared access. Supplying
            settings requests initialization access and changes timing.
        tx_timeout: Confirmation deadline in seconds, default one second.
            None disables it; zero is invalid.

    Requires Windows and the Vector XL driver. After a timeout, the next
    real transmission uses HIGHPRIO to replace pending hardware frames.
    """

    channel: int = 0
    tx_timeout: float | None = _DEFAULTS["tx_timeout"]
    backend: ClassVar[str] = "vector"

    @property
    def resolved_bitrate(self) -> BitrateConfig | None:
        """Calculated initialization timing, or None for shared channel access."""
        return None if self.bitrate is None else self.bitrate.resolve(self.backend)


@dataclass(frozen=True, slots=True, kw_only=True)
class J2534Config(_TimingConfig):
    """Configure a Windows SAE J2534 PassThru adapter.

    Args:
        dll_path: Vendor DLL path; None discovers it from the Windows registry.
        bitrate: Timing inputs; None selects 500 kbit/s Classic CAN.

    Use with J2534Adapter for raw CAN, or J2534NativeIsoTP.open for
    firmware-managed ISO-TP. The vendor DLL must support the Python process's
    architecture and concurrent native ISO-TP read/write calls.
    """

    dll_path: str | None = None
    backend: ClassVar[str] = "j2534"


AdapterConfig: TypeAlias = SocketCanConfig | PandaConfig | PeakConfig | VectorConfig | J2534Config
ConfigT = TypeVar("ConfigT", bound=AdapterConfig)


def _adapter_config(config: ConfigT | None, config_type: type[ConfigT]) -> ConfigT:
    if config is None:
        return config_type()
    if not isinstance(config, config_type):
        raise TypeError(f"config must be {config_type.__name__} or None")
    return config


class CanAdapter:
    """Open a CAN adapter immediately, using discovery when no config is supplied.

    Args:
        config: Reusable backend configuration; None discovers an adapter with
            Rust defaults. Named classes such as PeakAdapter select a backend.

    Construction blocks until hardware is ready, including inside an async
    function. Sends and receives are awaitable. Hardware is released when its
    last owner is dropped; transports, clients, and receivers retain ownership.
    Use await aclose() or async with for deterministic cleanup. Explicit closure
    invalidates dependent operations. A closed adapter cannot be reopened.

    Raises:
        ValueError: Explicit configuration is invalid.
        NotSupportedError: The backend is unavailable on this platform.
        AdapterError: Opening failed or discovery found no usable adapter.
    """

    def __init__(self, config: AdapterConfig | None = None) -> None:
        self._config = config
        self._handle = _rust.open_adapter(config)
        self._closed = False
        self._lifecycle = asyncio.Lock()

    @property
    def config(self) -> AdapterConfig | None:
        """Backend configuration, or None for automatic discovery."""
        return self._config

    @property
    def _native(self) -> _rust._CanAdapter:
        return self._handle

    async def open(self) -> Self:
        """Return this already-open adapter; raise RuntimeError after closure."""
        async with self._lifecycle:
            if self._closed:
                raise RuntimeError("adapter is closed")
        return self

    async def __aenter__(self) -> Self:
        """Return this already-open adapter for optional context management."""
        return await self.open()

    async def __aexit__(self, exc_type: object, exc: object, tb: object) -> None:
        """Close hardware even when the context exits with an exception."""
        await self.aclose()

    async def aclose(self) -> None:
        """Close this adapter and wake dependent operations; repeated calls are safe."""
        async with self._lifecycle:
            self._closed = True
            await self._handle.aclose()

    @property
    def closed(self) -> bool:
        """Whether this adapter has been explicitly closed."""
        return self._closed or self._handle.closed

    async def send(self, frame: Frame) -> None:
        """Send a frame and await the backend's transmit confirmation.

        Args:
            frame: Validated Classic, FD, or remote frame supported by the adapter.

        Raises:
            TimeoutError: A configured transmit deadline expired.
            TransmissionCancelledError: A queue clear invalidated this send.
            NotSupportedError: This backend cannot encode the frame.
            AdapterError: The hardware operation failed.

        Confirmation strength depends on the backend; some emulate ACKs at
        submission. Cancelling this await cannot undo an already-sent frame.
        """
        if not isinstance(frame, Frame):
            raise TypeError("frame must be a Frame")
        await self._native.send(frame)

    def recv(self) -> Receiver[Frame]:
        """Subscribe immediately to all frames, including transmit echoes.

        Returns:
            An async iterator of Frame objects. Create it before sending.
            Dropping it releases the subscription; aclose() releases it early.
            Slow consumers retain Rust's drop/lag behavior.
        """
        return Receiver(self._native.recv(), Frame._from_native)


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

    For firmware-managed ISO-TP, use J2534NativeIsoTP instead.
    """

    def __init__(self, config: J2534Config | None = None) -> None:
        super().__init__(_adapter_config(config, J2534Config))


def available_adapters() -> tuple[str, ...]:
    """Return compiled backend names; this does not probe connected hardware."""
    return tuple(_rust.available_adapters())


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
