# Python deployment

Build and publish from the repository's Nix development shell.

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

Run the following from the repository root to build all five targets:

```sh
nix develop
cd python
./build.sh
```

The development shell supplies a pinned Rust toolchain with all five target standard
libraries, LLVM/Clang and its cross-linkers, uv, and an Apple SDK fetched using
nixpkgs' pinned Apple URLs and checksum. It sets `SDKROOT` automatically. The
lockfile's release dependency group supplies Maturin, Zig, and Linux wheel repair
tooling.

Select individual targets with `--target linux-aarch64`, or several with
`--target linux-x86_64 linux-armv7 windows-x64`. Add `--dry-run` to preview commands.

Maturin uses Zig for Linux cross builds and xwin for Windows MSVC builds.
xwin downloads the Windows SDK/CRT and is subject to the
[Microsoft license described in its documentation](https://github.com/rust-cross/cargo-xwin).
Linux wheels enforce `manylinux_2_17` (glibc 2.17+). The build uses bundled libusb
and avoids host pkg-config libraries, retaining all platform-supported adapters.
Driver/DLL and USB permission requirements still apply at runtime.

Use alpha releases to test imports and hardware behavior on the target platforms
before a stable release. See [Maturin's cross-compilation guide](https://www.maturin.rs/distribution.html#cross-compiling)
and [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild).

Builds collect wheels and a source distribution in `python/dist/<version>/`.
Inspect the files and validate an upload:

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

## Documentation on GitHub Pages

The documentation site uses Sphinx, with its sources in `python/docs/`. Build it
locally from `python/` inside `nix develop`:

```sh
uv run --locked --group docs sphinx-build -b html -W --keep-going docs docs/_build/html
```

Open `docs/_build/html/index.html` to preview it. The site reuses `README.md`, this
guide, and the example files, and generates the API reference from the installed package's
docstrings. Its release version comes from `pyproject.toml` automatically.

The documentation workflow builds on pushes and deploys pushes to the repository's
default branch to GitHub Pages. In GitHub, select **Settings → Pages
→ Build and deployment → Source → GitHub Actions**. Deployment uses the built-in
GitHub token; no custom secret is needed. Under **Settings → Environments →
github-pages → Deployment branches and tags**, allow the default branch if
deployment branches are restricted.
