# SocketCAN

Open an existing Linux CAN interface with `SocketCanAdapter("can0")`, or use
`"vcan0"` for a virtual CAN interface. Bus timing is configured through the
operating system; opening the adapter does not change it.

## Configuration

The configuration is shared by the async and sync APIs.

```{eval-rst}
.. autoclass:: automotive.adapters.SocketCanConfig
   :members:
   :undoc-members:
```

## Asynchronous API

Send, receive, and lifecycle methods are inherited from
{class}`automotive.adapters.CanAdapter`.

```{eval-rst}
.. autoclass:: automotive.adapters.SocketCanAdapter
   :show-inheritance:
```

## Synchronous API

Blocking methods are inherited from {class}`automotive.sync.adapters.CanAdapter`.

```{eval-rst}
.. autoclass:: automotive.sync.adapters.SocketCanAdapter
   :show-inheritance:
```
