"""Shared Python resource and buffer conversions; no protocol logic."""

from __future__ import annotations

from collections.abc import AsyncIterator, Awaitable, Callable
from typing import Any, Generic, Protocol, Self, TypeAlias, TypeVar

from . import _native

Buffer: TypeAlias = bytes | bytearray | memoryview
T = TypeVar("T")


def buffer_bytes(value: Buffer) -> bytes:
    if isinstance(value, (bytes, bytearray)):
        return bytes(value)
    if isinstance(value, memoryview):
        if value.ndim == 1 and value.itemsize == 1 and value.c_contiguous:
            return value.tobytes()
        raise ValueError("memoryview must be contiguous, one-dimensional, and byte-oriented")
    raise TypeError("payload must be bytes, bytearray, or a byte-oriented memoryview")


def optional_bytes(value: Buffer | None) -> bytes | None:
    return None if value is None else buffer_bytes(value)


class _NativeResource(Protocol):
    def aclose(self) -> Awaitable[None]: ...

    @property
    def closed(self) -> bool: ...


R = TypeVar("R", bound=_NativeResource)


class AsyncResource(Generic[R]):
    """Context management shared by explicitly closable native resources."""

    def __init__(self, native: R) -> None:
        self._native: R = native

    async def __aenter__(self) -> Self:
        """Return this resource; opening is performed by its async factory."""
        if self.closed:
            raise RuntimeError("resource is closed")
        return self

    async def __aexit__(self, exc_type: object, exc: object, tb: object) -> None:
        """Release the resource even when its context exits with an exception."""
        await self.aclose()

    async def aclose(self) -> None:
        """Release hardware and wake pending operations; repeated calls are safe."""
        await self._native.aclose()

    @property
    def closed(self) -> bool:
        """Whether this resource or its owning parent has been closed."""
        return self._native.closed


class Receiver(AsyncIterator[T], Generic[T]):
    """An eager receive subscription with optional asynchronous cleanup.

    Create the receiver before sending a request. Dropping it releases the
    subscription. ``async with`` and aclose() support deterministic cleanup.
    A transport timeout raises ``TimeoutError``; another ``anext(receiver)``
    may continue reception. Closing the receiver or its owner ends iteration.
    """

    def __init__(self, native: _native._Receiver, convert: Callable[[Any], T]) -> None:
        self._native = native
        self._convert = convert

    def __aiter__(self) -> Self:
        """Return this subscription."""
        return self

    async def __anext__(self) -> T:
        """Await the next value, or raise StopAsyncIteration after closure."""
        value = await self._native.next()
        if value is None:
            raise StopAsyncIteration
        return self._convert(value)

    async def __aenter__(self) -> Self:
        """Return this already-subscribed receiver."""
        return self

    async def __aexit__(self, exc_type: object, exc: object, tb: object) -> None:
        """Close the receive subscription."""
        await self.aclose()

    async def aclose(self) -> None:
        """Wake a pending receive and release this subscription; idempotent."""
        await self._native.aclose()
