# Automotive for Python

An asyncio library for CAN, ISO-TP, and UDS, powered by the `automotive` Rust
crate. The distribution and import name are both `automotive`. Rust handles
adapter I/O, protocol framing, validation, and timing; Python provides reusable
configuration objects, typed values, context managers, and documented services.

Requires CPython 3.11+ and builds locally with maturin. This directory contains
the Python package, its Rust extension, examples, and Scapy ECU regression tests.

## Installation and development

From the repository root, with Rust and the adapter build dependencies available,
use the latest
[uv](https://docs.astral.sh/uv/getting-started/installation/) (at least 0.13.4):

```sh
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

The Nix development shell provides the native build dependencies. Enter it with
`nix develop` before the commands above. The Python extension always includes
every adapter supported on its platform: SocketCAN on Linux, panda and PEAK USB
adapters, and Vector and J2534 on Windows. Vendor drivers/DLLs and USB permissions
remain necessary. The Rust crate retains feature flags for Rust applications.

## Basic usage

Minimal setup retains the Rust defaults:

```python
import asyncio

from automotive.adapters import get_adapter
from automotive.protocols.uds import DataIdentifier, UDSClient
from automotive.transports.isotp import IsoTPAdapter


async def main():
    async with (
        await get_adapter() as adapter,
        IsoTPAdapter.from_id(adapter, 0x7A1) as transport,
    ):
        uds = UDSClient(transport)
        response = await uds.read_data_by_identifier(DataIdentifier.VIN)
        print(response)


asyncio.run(main())
```

## Configuration

Reusable configs expose advanced settings without Rust-style builders:

```python
from automotive.adapters import PeakConfig
from automotive.can import Identifier
from automotive.can.bitrate import BitrateSettings
from automotive.protocols.uds import UDSConfig
from automotive.transports.isotp import IsoTPConfig

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
# await get_adapter(adapter_config)
# IsoTPAdapter(adapter, transport_config)
# UDSClient(transport, uds_config)
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

The Python examples mirror every Rust example, including transmit timeout
recovery, advanced bitrate configuration, concurrent firmware queries, and
native J2534 device reuse. Read installed API documentation using
`uv run python -m pydoc automotive.protocols.uds`, or Python's `help()`.

## End-to-end regression tests

On Linux, prepare the kernel modules and `vcan0` from the repository root:

```sh
scripts/build_can_isotp.sh
scripts/set_up_vcan.sh
```

These scripts require sudo. Then run:

```sh
cd python
uv run pytest --run-vcan -q
```

The regression suite starts the existing Scapy ECU scripts, verifies readiness,
and closes/reaps them after each scenario. It exercises all 14 implemented UDS
service IDs, transfer round trips, suppression and pending responses, strict
validation, cancellation, and ISO-TP Classic/FD framing and flow control.
Tests run serially on `vcan0`; do not run the Rust and Python virtual-CAN suites
concurrently. Without `--run-vcan`, the hardware-dependent tests are skipped;
explicit runs fail if prerequisites are unavailable.
