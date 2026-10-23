# Synchronous API

```{eval-rst}
.. automodule:: automotive.sync
```

Configurations, CAN frames, UDS enums, result types, and exceptions are the same
objects documented in the other API pages. Both APIs run the same Rust protocol
implementation. Import paths mirror the async hierarchy:

```python
from automotive.sync.adapters import SocketCanAdapter
from automotive.sync.protocols.uds import DataIdentifier, UDSClient
from automotive.sync.transports.isotp import IsoTP
```

The synchronous reference appears alongside the async API in each package
section:

| Package | Reference |
| --- | --- |
| `automotive.sync.can` | [CAN frames and identifiers](can/index.md) |
| `automotive.sync.can.bitrate` | [Bitrate configuration](can/bitrate.md) |
| `automotive.sync.adapters` | {ref}`Adapters <sync-adapters>` |
| `automotive.sync.adapters.j2534` | {ref}`Native J2534 ISO-TP <sync-j2534>` |
| `automotive.sync.transports` | {ref}`Transport interface <sync-transports>` |
| `automotive.sync.transports.isotp` | {ref}`ISO-TP <sync-isotp>` |
| `automotive.sync.protocols.uds` | {ref}`UDS <sync-uds>` |
| `automotive.sync.errors` | [Exceptions](errors.md) |

## Receive subscriptions

```{eval-rst}
.. autoclass:: automotive.sync._base.Receiver
   :members:
```
