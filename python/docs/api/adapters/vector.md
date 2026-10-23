# Vector XL

Open a Vector XL channel on Windows. The Vector XL driver is required.
`VectorAdapter()` opens channel zero with shared access and retains its existing
bus timing. Pass a `VectorConfig` to select a channel or request initialization
access with explicit bitrate settings.

## Configuration

The configuration is shared by the async and sync APIs. The transmit timeout
defaults to one second; `tx_timeout=None` disables it.

```{eval-rst}
.. autoclass:: automotive.adapters.VectorConfig
   :members:
   :inherited-members:
   :undoc-members:
```

## Asynchronous API

Send, receive, and lifecycle methods are inherited from
{class}`automotive.adapters.CanAdapter`.

```{eval-rst}
.. autoclass:: automotive.adapters.VectorAdapter
   :show-inheritance:
```

## Synchronous API

Blocking methods are inherited from {class}`automotive.sync.adapters.CanAdapter`.

```{eval-rst}
.. autoclass:: automotive.sync.adapters.VectorAdapter
   :show-inheritance:
```
