"""Hardware discovery, explicit adapter selection, and reusable configuration.

Use ``async with await get_adapter() as adapter`` for discovery, or pass a
config to select a backend. Opening and closing run outside the Python thread.
SocketCAN uses OS-configured timing; USB adapters require their permissions and
Windows adapters require the vendor driver/DLL.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import ClassVar, TypeAlias

from .. import _native
from .._base import AsyncResource, Receiver
from ..can import Frame
from ..can.bitrate import BitrateConfig, BitrateSettings, _resolved

_DEFAULTS = _native.defaults()


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

    Use with get_adapter for raw CAN, or J2534NativeIsoTpTransport.open for
    firmware-managed ISO-TP. The vendor DLL must support the Python process's
    architecture and concurrent native ISO-TP read/write calls.
    """

    dll_path: str | None = None
    backend: ClassVar[str] = "j2534"


AdapterConfig: TypeAlias = SocketCanConfig | PandaConfig | PeakConfig | VectorConfig | J2534Config


class Adapter(AsyncResource[_native._Adapter]):
    """An open CAN adapter with asynchronous transmit confirmation and reception.

    Obtain one from get_adapter. The object retains its native resources until
    closed or released; prefer ``async with``. Transport/client references keep
    it alive, but explicitly closing it invalidates all dependent operations.
    """

    def __init__(self, native: _native._Adapter, config: AdapterConfig | None) -> None:
        super().__init__(native)
        self.config = config

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
            An async iterator of Frame objects. Create it before sending;
            close it after use. Slow consumers retain Rust's drop/lag behavior.
        """
        return Receiver(self._native.recv(), Frame._from_native)


async def get_adapter(config: AdapterConfig | None = None) -> Adapter:
    """Open an adapter without blocking Python's event loop.

    Args:
        config: Select a backend explicitly. None uses Rust's discovery order
            and defaults; unavailable devices are skipped during discovery.

    Returns:
        An open Adapter supporting ``async with`` and ``aclose()``.

    Raises:
        ValueError: Explicit configuration is invalid.
        NotSupportedError: The requested backend is not compiled for this platform.
        AdapterError: Opening failed, or discovery found no usable adapter.

    Examples:
        Requires an existing Linux interface::

            async with await get_adapter(SocketCanConfig(interface="vcan0")) as can:
                await can.send(Frame(id=0x123, data=bytes([1, 2])))
    """
    if config is not None and not isinstance(
        config, (SocketCanConfig, PandaConfig, PeakConfig, VectorConfig, J2534Config)
    ):
        raise TypeError("config must be an adapter config or None")
    return Adapter(await _native.open_adapter(config), config)


def available_adapters() -> tuple[str, ...]:
    """Return compiled backend names; this does not probe connected hardware."""
    return tuple(_native.available_adapters())


__all__ = [
    "Adapter",
    "AdapterConfig",
    "J2534Config",
    "PandaConfig",
    "PeakConfig",
    "SocketCanConfig",
    "VectorConfig",
    "available_adapters",
    "get_adapter",
]
