# Automotive for Python

[![PyPI](https://img.shields.io/pypi/v/automotive.svg)](https://pypi.org/project/automotive/)
[![Documentation](https://img.shields.io/badge/docs-GitHub%20Pages-blue.svg)](https://i-can-hack.github.io/automotive/)

A library for synchronous and asynchronous CAN, ISO-TP, and UDS, powered by the
`automotive` Rust crate. The distribution and import name are both `automotive`.
Rust handles adapter I/O, protocol framing, validation, and timing; Python
provides reusable configuration objects, typed values, context managers, and
documented services.

Requires CPython 3.11+ and builds locally with maturin. This directory contains
the Python package, its Rust extension, examples, and Scapy ECU regression tests.

Read the [Python documentation](https://i-can-hack.github.io/automotive/) for the
user guide, examples, and generated API reference.

## Installation and development

To test the alpha release from PyPI in another uv project:

```sh
uv add --prerelease allow automotive
```

To test the library from Git in another uv project, run this inside the Nix
development shell:

```sh
uv add "automotive @ git+https://github.com/I-CAN-hack/automotive.git#subdirectory=python"
```

For development, run the following from the repository root. The Nix shell
provides the required uv version, Rust toolchain, and native build dependencies:

```sh
nix develop
cd python
uv sync
uv run python examples/uds.py
uv check
uv format
```

`uv check` runs ty; `uv format` runs Ruff with default settings and a
120-character line length. `uv sync` installs the project and development tools;
the lockfile records their versions. Use `uv run maturin develop` after changing
Rust code when working directly in the installed development environment.
For Rust formatting, run `cargo fmt --all`.

The Python extension always includes every adapter supported on its platform:
SocketCAN on Linux, panda and PEAK USB adapters, and Vector and J2534 on Windows. Vendor drivers/DLLs and USB permissions
remain necessary. The Rust crate retains feature flags for Rust applications.

## Basic usage

Minimal setup retains the Rust defaults. Use `Adapter()` for automatic discovery,
`PeakAdapter()` for default PEAK settings, or `SocketCanAdapter("can0")` to select
a SocketCAN interface directly. Adapters open when entering `async with`;
`await adapter.open()` supports explicit lifecycle management:

```python
import asyncio

from automotive.adapters import Adapter
from automotive.protocols.uds import DataIdentifier, UDSClient
from automotive.transports.isotp import IsoTPAdapter


async def main():
    async with (
        Adapter() as adapter,
        IsoTPAdapter.from_id(adapter, 0x7A1) as transport,
    ):
        uds = UDSClient(transport)
        response = await uds.read_data_by_identifier(DataIdentifier.VIN)
        print(response)


asyncio.run(main())
```

### Synchronous usage

Import blocking adapter, transport, and client classes from `automotive.sync`.
Use ordinary `with` blocks, method calls, and receive iterators:

```python
from automotive.protocols.uds import DataIdentifier
from automotive.sync import IsoTPAdapter, SocketCanAdapter, UDSClient

with SocketCanAdapter("can0") as adapter, IsoTPAdapter.from_id(adapter, 0x7A1) as transport:
    uds = UDSClient(transport)
    print(uds.read_data_by_identifier(DataIdentifier.VIN))
    with uds.suppress_positive_responses():
        uds.tester_present()
```

`Adapter()`, `PeakAdapter(config)`, and the other named adapter classes are also
available in `automotive.sync`. Both APIs share configuration classes, frames,
enums, result types, and exceptions. Configs can be imported from their existing
modules or from `automotive.sync`.

For explicit cleanup, use `close()`; adapters also support `open()`. `recv()`
subscribes immediately and returns an iterator for `for` or `next()`, supporting
`with` and `close()`. Close raw ISO-TP receivers before making UDS requests on
the same transport. Native J2534 transports and reusable device handles have
synchronous counterparts too.

One background event loop runs the existing async implementation and shuts down
at process exit. Calls block the calling thread, so use the async API inside
asyncio applications. Suppression scopes in the synchronous API are local to the
calling thread and context. Ctrl+C interrupts a blocking wait; an active Rust UDS
exchange still drains before the next request, matching async cancellation.

## Configuration

Reusable configs expose advanced settings without Rust-style builders:

```python
import asyncio

from automotive.adapters import PeakAdapter, PeakConfig
from automotive.can import Identifier
from automotive.can.bitrate import BitrateSettings
from automotive.protocols.uds import DataIdentifier, UDSClient, UDSConfig
from automotive.transports.isotp import IsoTPAdapter, IsoTPConfig

adapter_config = PeakConfig(
    bitrate=BitrateSettings(
        bitrate=500_000, sample_point=0.8,
        data_bitrate=2_000_000, data_sample_point=0.8,
    ),
    tx_timeout=1.0,
)
transport_config = IsoTPConfig(
    tx_id=Identifier.extended(0x18DA10F1),
    fd=True, max_dlen=32, block_size=8,
    receive_separation_time_min=0.0003,
)
uds_config = UDSConfig(request_timeout=10.0, strict=True)


async def main():
    async with (
        PeakAdapter(adapter_config) as adapter,
        IsoTPAdapter(adapter, transport_config) as transport,
    ):
        uds = UDSClient(transport, uds_config)
        print(await uds.read_data_by_identifier(DataIdentifier.VIN))


asyncio.run(main())
```

All Python durations are seconds. Omitting settings retains Rust defaults;
explicit bitrate settings start without presets, so a nominal bitrate alone
selects Classic CAN. Vector retains shared access and existing timing when no
bitrate settings are supplied. `resolved_bitrate` reports the calculated timing;
SocketCAN selects an existing OS-configured interface.

Payload inputs accept `bytes`, `bytearray`, and contiguous byte-oriented
`memoryview` objects; payload results are `bytes`. Named codes are Python
`IntEnum` members, raw numbers are accepted, and unknown received codes are
preserved. Invalid arguments use Python's `ValueError`/`TypeError`/`OverflowError`;
deadlines use `TimeoutError`; disconnects use `ConnectionError`. Protocol
exceptions retain NRCs or full response bytes. See `automotive.errors`.

Receive subscriptions are created by `recv()` immediately: create one before
sending, then use `async for`, `anext()`, and `aclose()`. Hardware, transports,
and receivers support `async with`. Close raw ISO-TP receivers before using UDS
on the same transport; raw CAN monitoring can run alongside UDS.
UDS exchanges on a transport are serialized. A cancelled queued call does not
transmit; an active cancelled exchange drains under the lock before the next
call starts. Closing the parent interrupts dependent operations. Set
`request_timeout` when an overall bound is needed, including repeated NRC 0x78.

Use an ordinary context manager to suppress positive responses. Suppression
supports nested scopes, restores its previous setting on exit, and is local to
the current async task. Tasks created inside the scope inherit its context;
already-running tasks are unaffected. Negative responses still raise errors.

```python
with uds.suppress_positive_responses():
    await uds.tester_present()
await uds.tester_present()  # Requires a positive response again.
```

## Examples and API documentation

The async Python examples mirror every Rust example, including CAN transmission,
advanced bitrate configuration, and concurrent firmware queries. Blocking CAN
send/receive, ISO-TP, and UDS examples are in `examples/sync/`.
Read installed API documentation using
`uv run python -m pydoc automotive.protocols.uds`, or Python's `help()`.

Run `uv run python examples/can_sender.py` to send one Classic CAN frame.
Use `uv run python examples/sync/can_sender.py` for the synchronous version.

## End-to-end regression tests

On Linux, create `vcan0` from the repository root:

```sh
scripts/set_up_vcan.sh
```

The setup script requires sudo. Then run:

```sh
cd python
uv run pytest --run-vcan -q
```

The regression suite starts the existing Scapy ECU scripts, verifies readiness,
and closes/reaps them after each scenario. It exercises all 14 implemented UDS
service IDs through both APIs, transfer round trips, suppression and pending
responses, strict validation, cancellation, and ISO-TP Classic/FD framing and
flow control.
Tests run serially on `vcan0`; do not run the Rust and Python virtual-CAN suites
concurrently. Without `--run-vcan`, the hardware-dependent tests are skipped;
explicit runs fail if prerequisites are unavailable.
