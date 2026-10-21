# Automotive for Python

An asyncio library for CAN, ISO-TP, and UDS, powered by Rust. Start with sensible
defaults, then use reusable configuration objects when you need more control.

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
