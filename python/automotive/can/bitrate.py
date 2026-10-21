"""Reusable bitrate requests and the timing values calculated by Rust."""

from __future__ import annotations

from dataclasses import dataclass

from .. import _native


@dataclass(frozen=True, slots=True, kw_only=True)
class AdapterBitTiming:
    """Resolved timing for one CAN phase.

    Attributes:
        brp: Bitrate prescaler.
        tseg1: Time segment 1, in time quanta.
        tseg2: Time segment 2, in time quanta.
        sjw: Synchronization jump width, in time quanta.
        bitrate: Actual calculated bitrate, in bits per second.
        sample_point: Actual normalized sample point between zero and one.
    """

    brp: int
    tseg1: int
    tseg2: int
    sjw: int
    bitrate: int
    sample_point: float


@dataclass(frozen=True, slots=True)
class BitrateConfig:
    """Calculated nominal timing and optional CAN-FD data-phase timing.

    Obtain this record with ``BitrateSettings.resolve(backend)`` or an adapter
    config's ``resolved_bitrate`` property. It reports Rust's calculation.
    """

    nominal: AdapterBitTiming
    data: AdapterBitTiming | None = None


@dataclass(frozen=True, slots=True, kw_only=True)
class BitrateSettings:
    """Bitrate-builder inputs, resolved for the selected adapter by Rust.

    Args:
        bitrate: Target nominal bitrate in bits per second.
        sample_point: Normalized nominal sample point; None uses Rust's
            bitrate-dependent default.
        brp: Direct nominal prescaler, used with tseg1 and tseg2.
        tseg1: Direct nominal segment 1, in time quanta.
        tseg2: Direct nominal segment 2, in time quanta.
        sjw: Nominal synchronization jump width, in time quanta.
        data_bitrate: Optional CAN-FD data bitrate in bits per second.
        data_sample_point: Normalized data-phase sample point.
        data_sjw: Data-phase synchronization jump width, in time quanta.
        max_bitrate_error: Maximum bitrate error in hundredths of one percent.
            None retains Rust's 50 (0.5%) default.

    Supply either a target bitrate or complete direct nominal timing.
    Explicit settings start without preset timing: setting only ``bitrate``
    produces Classic CAN. Omitting settings on an adapter retains that
    adapter's defaults, including its default FD data bitrate where applicable.
    """

    bitrate: int | None = None
    sample_point: float | None = None
    brp: int | None = None
    tseg1: int | None = None
    tseg2: int | None = None
    sjw: int | None = None
    data_bitrate: int | None = None
    data_sample_point: float | None = None
    data_sjw: int | None = None
    max_bitrate_error: int | None = None

    def resolve(self, backend: str) -> BitrateConfig:
        """Calculate timing using this backend's Rust capabilities.

        Args:
            backend: "panda", "peak", "vector", or "j2534".

        Returns:
            Calculated nominal timing and optional data timing.

        Raises:
            ValueError: The requested timing cannot be represented.
            NotSupportedError: This build cannot configure the backend.
        """
        return _resolved(backend, self)


def _resolved(backend: str, settings: BitrateSettings | None) -> BitrateConfig:
    value = _native.resolve_bitrate(backend, settings)
    return BitrateConfig(
        AdapterBitTiming(**value["nominal"]), None if value["data"] is None else AdapterBitTiming(**value["data"])
    )
