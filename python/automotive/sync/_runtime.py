"""Run the existing asynchronous façade on one lazily started background loop."""

from __future__ import annotations

import asyncio
import atexit
from collections.abc import Awaitable, Callable, Coroutine, Iterator
from concurrent.futures import Future
from threading import Lock, Thread, current_thread
from typing import Any, Generic, ParamSpec, Protocol, Self, TypeVar

from .._base import Receiver as _AsyncReceiver

P = ParamSpec("P")
T = TypeVar("T")


class _Runtime:
    def __init__(self) -> None:
        self._lock = Lock()
        self._ready: Future[asyncio.AbstractEventLoop] = Future()
        self._thread: Thread | None = None
        self._closed = False

    @staticmethod
    def _serve(ready: Future[asyncio.AbstractEventLoop]) -> None:
        try:
            with asyncio.Runner() as runner:
                loop = runner.get_loop()
                ready.set_result(loop)
                loop.run_forever()
        except BaseException as error:
            if not ready.done():
                ready.set_exception(error)
            else:
                raise

    def submit(self, operation: Callable[[], Coroutine[Any, Any, T]]) -> T:
        with self._lock:
            if self._closed:
                raise RuntimeError("synchronous automotive runtime is closed")
            if current_thread() is self._thread:
                raise RuntimeError("a synchronous automotive call cannot run on its own event loop")
            if self._thread is None:
                self._thread = Thread(target=self._serve, args=(self._ready,), name="automotive-sync", daemon=True)
                self._thread.start()
            loop = self._ready.result()
            future = asyncio.run_coroutine_threadsafe(operation(), loop)
        try:
            while True:
                try:
                    # A bounded wait also lets Windows process Ctrl+C while receiving.
                    return future.result(timeout=0.1)
                except TimeoutError:
                    if future.done():
                        return future.result()
        except BaseException:
            future.cancel()
            raise

    def close(self) -> None:
        with self._lock:
            if self._closed:
                return
            self._closed = True
            thread = self._thread
            if thread is None:
                return
            loop = self._ready.result()
            loop.call_soon_threadsafe(loop.stop)
        thread.join()


_runtime = _Runtime()
atexit.register(_runtime.close)


def run(operation: Callable[P, Awaitable[T]], /, *args: P.args, **kwargs: P.kwargs) -> T:
    """Await an operation on the shared loop, retaining the caller's context."""

    async def invoke() -> T:
        return await operation(*args, **kwargs)

    return _runtime.submit(invoke)


def call(operation: Callable[P, T], /, *args: P.args, **kwargs: P.kwargs) -> T:
    """Run synchronous setup on the same loop as the resource's async methods."""

    async def invoke() -> T:
        return operation(*args, **kwargs)

    return _runtime.submit(invoke)


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
