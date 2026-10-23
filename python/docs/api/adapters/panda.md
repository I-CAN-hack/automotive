# panda

Open a panda adapter over USB on Linux, macOS, or Windows. `PandaAdapter()` uses
the default bus timing; pass a `PandaConfig` to customize it. USB access
permissions are required.

## Configuration

The configuration is shared by the async and sync APIs. Omitted bitrate settings
use 500 kbit/s nominal and 2 Mbit/s data timing, with 80% sample points.

```{eval-rst}
.. autoclass:: automotive.adapters.PandaConfig
   :members:
   :inherited-members:
   :undoc-members:
```

## Asynchronous API

Send, receive, and lifecycle methods are inherited from
{class}`automotive.adapters.CanAdapter`.

```{eval-rst}
.. autoclass:: automotive.adapters.PandaAdapter
   :show-inheritance:
```

## Synchronous API

Blocking methods are inherited from {class}`automotive.sync.adapters.CanAdapter`.

```{eval-rst}
.. autoclass:: automotive.sync.adapters.PandaAdapter
   :show-inheritance:
```
