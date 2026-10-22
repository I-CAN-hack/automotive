# Synchronous API

```{eval-rst}
.. automodule:: automotive.sync
```

Configurations, CAN frames, UDS enums, result types, and exceptions are the same
objects documented in the other API pages. Both APIs run the same Rust protocol
implementation.

## Adapters

Named adapters inherit the blocking lifecycle, send, and receive methods from
`Adapter`. Their constructors accept the same configuration as the async API.

```{eval-rst}
.. autoclass:: automotive.sync.Adapter
   :members:
   :inherited-members:

.. autoclass:: automotive.sync.SocketCanAdapter

.. autoclass:: automotive.sync.PandaAdapter

.. autoclass:: automotive.sync.PeakAdapter

.. autoclass:: automotive.sync.VectorAdapter

.. autoclass:: automotive.sync.J2534Adapter
```

## ISO-TP transports

```{eval-rst}
.. autoclass:: automotive.sync.IsoTPAdapter
   :members:
   :inherited-members:

.. autoclass:: automotive.sync.J2534NativeIsoTpTransport
   :members: open, new_on_device, into_device, adapter_config

.. autoclass:: automotive.sync.J2534Device
   :members:
   :inherited-members:
```

## UDS client

```{eval-rst}
.. autoclass:: automotive.sync.UDSClient
   :members:
```

## Receive subscriptions

```{eval-rst}
.. autoclass:: automotive.sync.Receiver
   :members:
```
