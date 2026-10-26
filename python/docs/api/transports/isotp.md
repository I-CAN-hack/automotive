# ISO-TP

Software ISO-TP exchanges complete messages over a CAN adapter. `IsoTPConfig`
is shared by the async and sync implementations.

With `fd=True`, BRS defaults to enabled when the adapter reports a configured
data bitrate, or when timing is unknown or managed externally (such as SocketCAN
or shared Vector channels). Set `brs=True` or `brs=False` to override it for both
data and flow-control frames. Enabling BRS requires CAN FD and adapter support.
Both APIs use the same setting:
`IsoTPConfig(tx_id=0x7A1, fd=True, brs=True)`.

When transmit STmin is zero, both APIs queue consecutive frames in bounded
batches within the peer's block limit. Every frame's transmit confirmation is
awaited before completing the block or transfer, without a timer delay between
frames. Batching is enabled by default. For adapters that reorder queued
frames, use `IsoTPConfig(tx_id=0x7A1, batch_send=False)` in either API to
confirm each consecutive frame before submitting the next one.

## Asynchronous API

```{eval-rst}
.. automodule:: automotive.transports.isotp
   :members:
   :inherited-members:
   :undoc-members:
```

(sync-isotp)=
## Synchronous API

```{eval-rst}
.. autoclass:: automotive.sync.transports.isotp.IsoTP
   :members:
   :inherited-members:
```
