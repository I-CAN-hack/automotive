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

## Adapters

Named adapters inherit the blocking lifecycle, send, and receive methods from
`CanAdapter`. Constructors open devices immediately and accept the same
configuration as the async API. Resources are released when their last owner is
dropped; explicit close() and context managers remain optional.

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

## Transports

`UDSClient` accepts the shared `Transport` base independently of the underlying
protocol. Software ISO-TP and native J2534 are concrete implementations.

```{eval-rst}
.. autoclass:: automotive.sync.transports.Transport
   :members:
   :inherited-members:

.. autoclass:: automotive.sync.transports.isotp.IsoTP
   :members:
   :inherited-members:

.. autoclass:: automotive.sync.adapters.j2534.J2534NativeIsoTP
   :members: open, new_on_device, into_device, adapter_config

.. autoclass:: automotive.sync.adapters.j2534.J2534Device
   :members:
   :inherited-members:
```

## UDS client

```{eval-rst}
.. autoclass:: automotive.sync.protocols.uds.UDSClient
   :members:
```

## Receive subscriptions

```{eval-rst}
.. autoclass:: automotive.sync._base.Receiver
   :members:
```
