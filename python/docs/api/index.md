# API reference

The reference is generated from the installed package's docstrings, signatures,
and type annotations. The Python package delegates adapter I/O and protocol
processing to Rust.

The reference follows the Python package layout: CAN values and timing in
`automotive.can`, hardware backends in `automotive.adapters`, message transports
in `automotive.transports`, and diagnostic clients in `automotive.protocols`.
Each section also documents its counterparts under `automotive.sync`; shared
configuration and value types are documented once.

```{toctree}
:maxdepth: 2

can/index
adapters/index
transports/index
protocols/index
sync
errors
```
