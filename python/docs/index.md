# Automotive for Python

A library for synchronous and asynchronous CAN, ISO-TP, and UDS, powered by Rust.
Start with sensible defaults, then use reusable configuration objects when you
need more control. The modules under `automotive.sync` mirror the asyncio API,
including `automotive.sync.adapters`, `automotive.sync.transports.isotp`, and
`automotive.sync.protocols.uds`.

Requires CPython 3.11 or newer. Install into a uv project:

```sh
uv add automotive
```

To include prereleases while testing, use `uv add --prerelease allow automotive`.
Platform wheels include every supported adapter; vendor drivers and USB access
permissions are still needed for the hardware you use.

The [user guide](guide.md) covers setup, configuration, and resource management.
The [examples](examples.md) show complete programs, and the
[API reference](api/index.md) documents the public Python classes and methods.

```{toctree}
:maxdepth: 2

guide
examples
api/index
development
deployment
```
