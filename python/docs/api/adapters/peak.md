# PEAK PCAN-USB FD

Open a PEAK PCAN-USB FD adapter directly over USB on Linux, macOS, or Windows.
`PeakAdapter()` uses the default bus timing; pass a `PeakConfig` to customize
timing and the transmit timeout. USB access permissions are required.

Both APIs use a 1 ms USB receive timeout to limit how long idle reads can delay
outgoing frames. Sending still waits for CAN transmit confirmation.

## Configuration

The configuration is shared by the async and sync APIs. Omitted bitrate settings
use 500 kbit/s nominal and 2 Mbit/s data timing, with 80% sample points. The
transmit timeout defaults to one second; `tx_timeout=None` disables it.

```{eval-rst}
.. autoclass:: automotive.adapters.PeakConfig
   :members:
   :inherited-members:
   :undoc-members:
```

## Asynchronous API

Send, receive, and lifecycle methods are inherited from
{class}`automotive.adapters.CanAdapter`.

```{eval-rst}
.. autoclass:: automotive.adapters.PeakAdapter
   :show-inheritance:
```

## Synchronous API

Blocking methods are inherited from {class}`automotive.sync.adapters.CanAdapter`.

```{eval-rst}
.. autoclass:: automotive.sync.adapters.PeakAdapter
   :show-inheritance:
```
