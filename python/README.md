# Automotive for Python

An asyncio library for CAN, ISO-TP, and UDS, powered by the `automotive` Rust
crate. The distribution and import name are both `automotive`. Rust handles
adapter I/O, protocol framing, validation, and timing; Python provides reusable
configuration objects, typed values, context managers, and documented services.

Requires CPython 3.11+ and builds locally with maturin. This directory contains
the Python package, its Rust extension, examples, and Scapy ECU regression tests.

## Installation and development

To test the alpha release from PyPI in another uv project:

```sh
uv add --prerelease allow automotive
```

To test the library in another uv project, with Rust and the adapter
build dependencies available:

```sh
uv add "automotive @ git+https://github.com/I-CAN-hack/automotive.git#subdirectory=python"
```

From the repository root, with Rust and the adapter build dependencies available,
use the latest
[uv](https://docs.astral.sh/uv/getting-started/installation/).

With Nix, first run `nix develop` from the repository root. The flake provides
the required uv version and native build dependencies, taking precedence over an older uv
on your normal PATH.

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

## Publishing to PyPI

Set the Python release version in `pyproject.toml`. Both `build.sh` and `deploy.sh`
read it automatically: `build.sh` builds the artifacts, and `deploy.sh` uploads
them with `uv publish`. No CI publishing is configured.
Release wheels use CPython's stable ABI starting at 3.11, so each
platform needs one wheel for regular CPython 3.11 and newer. Free-threaded Python
builds need separate wheels and are not included here.

The supported release targets are:

| Build target | Rust target | Platform |
| --- | --- | --- |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` | 64-bit Linux x86 |
| `linux-aarch64` | `aarch64-unknown-linux-gnu` | 64-bit Linux ARM |
| `linux-armv7` | `armv7-unknown-linux-gnueabihf` | 32-bit Raspberry Pi 2+ |
| `macos-arm64` | `aarch64-apple-darwin` | Apple Silicon, macOS 11+ |
| `windows-x64` | `x86_64-pc-windows-msvc` | 64-bit Windows x86 |

On Nix, run the following from the repository root to build all five targets:

```sh
nix develop
cd python
./build.sh
```

The development shell supplies a pinned Rust toolchain with all five target standard
libraries, LLVM/Clang and its cross-linkers, uv, and an Apple SDK fetched using
nixpkgs' pinned Apple URLs and checksum. It sets `SDKROOT` automatically, so no
rustup installation or manual SDK extraction is needed. The lockfile's release dependency
group supplies Maturin, Zig, and Linux wheel repair tooling.

Select individual targets with `--target linux-aarch64`, or several with
`--target linux-x86_64 linux-armv7 windows-x64`. Add `--dry-run` to preview commands.

Outside Nix, install Rust with the target standard libraries, LLVM/Clang
(including libclang and llvm-dlltool), and uv. For a rustup toolchain:

```sh
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu \
    armv7-unknown-linux-gnueabihf aarch64-apple-darwin x86_64-pc-windows-msvc
```

Maturin uses Zig for Linux cross builds and xwin for Windows MSVC builds.
xwin downloads the Windows SDK/CRT and is subject to the
[Microsoft license described in its documentation](https://github.com/rust-cross/cargo-xwin).
Linux wheels enforce `manylinux_2_17` (glibc 2.17+). The build uses bundled libusb
and avoids host pkg-config libraries, retaining all platform-supported adapters.
Driver/DLL and USB permission requirements still apply at runtime.

Outside the Nix shell, macOS ARM64 can be built on an Apple Silicon Mac or
cross-compiled from Linux with an extracted Apple SDK. For the latter, set `SDKROOT`:

```sh
SDKROOT=/path/to/MacOSX.sdk ./build.sh --target macos-arm64
```

Use alpha releases to test imports and hardware behavior on the target platforms
before a stable release. See [Maturin's cross-compilation guide](https://www.maturin.rs/distribution.html#cross-compiling)
and [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild).

Builds collect wheels and a source distribution in `python/dist/<version>/`. The
source distribution allows installation on other supported systems with Rust
and native build prerequisites. Inspect the files and validate an upload:

```sh
./deploy.sh --dry-run
```

Create a PyPI account with 2FA and a verified email, then create an
[API token](https://pypi.org/help/#apitoken). The first upload creates the project;
use an account-scoped token initially, then a project-scoped token for later
releases. Provide the token through `UV_PUBLISH_TOKEN` in your shell or secret
manager, and run:

```sh
./deploy.sh
```

To rehearse against TestPyPI, use its separate account/token and upload endpoint:

```sh
./deploy.sh --publish-url https://test.pypi.org/legacy/ \
    --check-url https://test.pypi.org/simple/
```

`deploy.sh` selects only the current version's directory and forwards its arguments
to `uv publish`. If you build with `--dist-dir PATH`, publish that directory directly
with `uv publish 'PATH/*'`.
See the [uv publishing guide](https://docs.astral.sh/uv/guides/package/#publishing-your-package).
