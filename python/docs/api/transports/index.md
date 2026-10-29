# Transports

`UDSClient` accepts a `Transport`, which exchanges complete protocol data units
independently of the underlying protocol. Software `IsoTP`, `DoIP`, and
`J2534NativeIsoTP` share this interface. DoIP communicates over IPv4/IPv6 sockets
without a CAN adapter.

Software ISO-TP lives in `automotive.transports.isotp`. The native J2534
implementation lives in `automotive.adapters.j2534` and is documented under
[adapters](../adapters/j2534.md).

```{toctree}
:maxdepth: 1

isotp
doip
```

## Asynchronous API

```{eval-rst}
.. automodule:: automotive.transports
   :members:
   :inherited-members:
```

(sync-transports)=
## Synchronous API

`automotive.sync.transports.Transport` provides the matching blocking interface
for synchronous ISO-TP, native J2534, and diagnostic clients.

```{eval-rst}
.. autoclass:: automotive.sync.transports.Transport
   :members:
   :inherited-members:
```
