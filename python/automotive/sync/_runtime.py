"""Run the existing asynchronous façade on one lazily started background loop."""

from __future__ import annotations

import asyncio
import atexit
from collections.abc import Awaitable, Callable, Coroutine
from concurrent.futures import Future
from threading import Lock, Thread, current_thread
from typing import Any, ParamSpec, TypeVar

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
