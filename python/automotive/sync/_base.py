"""Blocking resource management and receive iteration shared across the façade."""

from __future__ import annotations

from collections.abc import Awaitable, Iterator
from typing import Generic, Protocol, Self, TypeVar

from .._base import Receiver as _AsyncReceiver
from ._runtime import run

T = TypeVar("T")


class _Closable(Protocol):
    def aclose(self) -> Awaitable[None]: ...

    @property
    def closed(self) -> bool: ...


R = TypeVar("R", bound=_Closable)


class Resource(Generic[R]):
    """Ordinary context management for an existing asynchronous resource."""

    def __init__(self, resource: R) -> None:
        self._async = resource

    def __enter__(self) -> Self:
        """Return this open resource; raise RuntimeError after closure."""
        if self.closed:
            raise RuntimeError("resource is closed")
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> None:
        """Close this resource even if the body raises an exception."""
        self.close()

    def close(self) -> None:
        """Release the resource and wake pending operations; idempotent."""
        run(self._async.aclose)

    @property
    def closed(self) -> bool:
        """Whether this resource or its owning parent has been closed."""
        return self._async.closed


class Receiver(Iterator[T], Generic[T]):
    """An eager receive subscription supporting ordinary iteration and with.

    Create the subscription before sending. Each next() blocks until a value,
    a protocol timeout, or closure. A timeout raises TimeoutError and leaves the
    subscription usable. Closing the receiver or its owner ends iteration.
    Dropping the receiver releases the subscription. A with block or close()
    supports deterministic cleanup.
    """

    def __init__(self, receiver: _AsyncReceiver[T]) -> None:
        self._async = receiver

    def __iter__(self) -> Self:
        """Return this subscription."""
        return self

    def __next__(self) -> T:
        """Block for the next value, or raise StopIteration after closure."""
        try:
            return run(self._async.__anext__)
        except StopAsyncIteration:
            raise StopIteration from None

    def __enter__(self) -> Self:
        """Return this already-subscribed receiver."""
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> None:
        """Close the receive subscription."""
        self.close()

    def close(self) -> None:
        """Wake a pending receive and release this subscription; idempotent."""
        run(self._async.aclose)
