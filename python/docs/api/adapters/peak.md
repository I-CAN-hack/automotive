# PEAK PCAN-USB FD

Open a PEAK PCAN-USB FD adapter directly over USB on Linux, macOS, or Windows.
`PeakAdapter()` uses the default bus timing; pass a `PeakConfig` to customize
timing and the transmit timeout. USB access permissions are required.

## Configuration

The configuration is shared by the async and sync APIs. Omitted bitrate settings
use 500 kbit/s nominal and 2 Mbit/s data timing, with 80% sample points. The
transmit timeout defaults to one second; `tx_timeout=None` disables it.

`detach_kernel_driver` defaults to `False`. Opening fails if a kernel driver
such as `peak_usb` already owns the USB interface. Set
`PeakConfig(detach_kernel_driver=True)` to allow detaching that driver while
the raw-USB adapter is open, interrupting SocketCAN access. The driver is
reattached when the adapter is released. Both APIs share this setting, and
automatic discovery leaves kernel drivers attached.

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
