# Adapters

Use `CanAdapter()` for discovery, or choose a backend explicitly. Constructors open
hardware immediately and block until ready. Sending and receiving are async.
Hardware is released when its last owner is dropped; `aclose()` and `async with`
remain available for deterministic cleanup.

```{toctree}
:maxdepth: 1

j2534
```

## Asynchronous API

```{eval-rst}
.. automodule:: automotive.adapters
   :members:
   :inherited-members:
   :undoc-members:
```

(sync-adapters)=
## Synchronous API

Named adapters in `automotive.sync.adapters` inherit the blocking lifecycle,
send, and receive methods from `CanAdapter`. Constructors open devices
immediately and accept the same configuration as the async API. Resources are
released when their last owner is dropped; `close()` and context managers
remain available for deterministic cleanup.

```{eval-rst}
.. autoclass:: automotive.sync.adapters.CanAdapter
   :members:
   :inherited-members:

.. autoclass:: automotive.sync.adapters.SocketCanAdapter

.. autoclass:: automotive.sync.adapters.PandaAdapter

.. autoclass:: automotive.sync.adapters.PeakAdapter

.. autoclass:: automotive.sync.adapters.VectorAdapter

.. autoclass:: automotive.sync.adapters.J2534Adapter
```
