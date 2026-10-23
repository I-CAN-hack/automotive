# Adapters

## Supported adapters

| Adapter | Platforms | Connection |
| --- | --- | --- |
| [SocketCAN](socketcan.md) | Linux | OS-configured CAN and virtual CAN interfaces |
| [panda](panda.md) | Linux, macOS, Windows | USB |
| [PEAK PCAN-USB FD](peak.md) | Linux, macOS, Windows | USB |
| [Vector XL](vector.md) | Windows | Vector XL driver |
| [J2534](j2534.md) | Windows | Vendor PassThru DLL; raw CAN and native ISO-TP |

Each adapter page documents its configuration and both async and sync classes.

```{toctree}
:maxdepth: 1
:hidden:

socketcan
panda
peak
vector
j2534
```

## Discovery and shared interface

Use `CanAdapter()` for discovery, or choose a backend from the table above.
`available_adapters()` reports the backends compiled for the current platform;
it does not probe connected hardware. Constructors open hardware immediately and
block until ready in both APIs. The named adapter classes inherit the send,
receive, and lifecycle methods documented below.

Hardware is released when its last owner is dropped. For deterministic cleanup,
use `aclose()` or `async with` in the async API, or `close()` or `with` in the
sync API.

`await adapter.recv_for(duration)` collects CAN frames for the given number of
seconds and returns them as a list in receive order, including transmit echoes.
Use `adapter.recv_for(duration)` in the sync API. Each call closes its own
subscription on return and leaves existing `recv()` subscriptions unaffected.
Reception starts when awaited (or called in the sync API) and finishes early
if the adapter closes.

## Asynchronous API

```{eval-rst}
.. py:module:: automotive.adapters

.. autodata:: automotive.adapters.AdapterConfig

.. autofunction:: automotive.adapters.available_adapters

.. autoclass:: automotive.adapters.CanAdapter
   :members:
   :inherited-members:
   :undoc-members:
```

(sync-adapters)=
## Synchronous API

Named adapters in `automotive.sync.adapters` inherit the blocking lifecycle,
send, and receive methods from `CanAdapter` and accept the same configuration
as the async API.

```{eval-rst}
.. autoclass:: automotive.sync.adapters.CanAdapter
   :members:
   :inherited-members:
```
