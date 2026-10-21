"""Build the release wheel matrix locally without uploading anything.

Run ``uv run --group release python build_wheels.py --help`` from this directory.
Upload the resulting artifacts with uv publish; --dry-run previews build commands.
Cross builds require Rust target standard libraries, LLVM, and, for macOS, SDKROOT.
"""

from __future__ import annotations

import argparse
import os
import shlex
import subprocess
import sys
import tomllib
from pathlib import Path

PYTHON_DIR = Path(__file__).resolve().parent
REPO_DIR = PYTHON_DIR.parent
TARGETS = {
    "linux-x86_64": "x86_64-unknown-linux-gnu",
    "linux-aarch64": "aarch64-unknown-linux-gnu",
    "linux-armv7": "armv7-unknown-linux-gnueabihf",
    "macos-arm64": "aarch64-apple-darwin",
    "windows-x64": "x86_64-pc-windows-msvc",
}


def run(command: list[str], *, dry_run: bool, env: dict[str, str] | None = None) -> None:
    """Display a command and execute it with inherited output unless previewing."""
    print(shlex.join(command), flush=True)
    if not dry_run:
        subprocess.run(command, cwd=PYTHON_DIR, env=env, check=True)


def version() -> str:
    """Read the release version and reject mismatched Rust/Python manifests."""
    with (REPO_DIR / "Cargo.toml").open("rb") as stream:
        rust_version = tomllib.load(stream)["package"]["version"]
    with (PYTHON_DIR / "Cargo.toml").open("rb") as stream:
        python_version = tomllib.load(stream)["package"]["version"]
    if rust_version != python_version:
        raise ValueError("Rust and Python Cargo.toml versions must match")
    return rust_version


def build(targets: list[str], output: Path, *, dry_run: bool) -> None:
    """Build the sdist and stable-ABI wheels, retaining every artifact for review."""
    selected = list(TARGETS) if "all" in targets else list(dict.fromkeys(targets))
    if "macos-arm64" in selected and sys.platform != "darwin" and not dry_run:
        sdk = os.environ.get("SDKROOT")
        if not sdk or not Path(sdk).is_dir():
            raise ValueError("Set SDKROOT to an extracted macOS SDK before cross-compiling macOS ARM64")
    if not dry_run:
        output.mkdir(parents=True, exist_ok=True)
    maturin = ["uv", "run", "--locked", "--group", "release", "maturin"]
    for target in selected:
        env = os.environ.copy()
        # libusb1-sys falls back to its bundled C source. Avoid linking host
        # libusb/libudev or accidentally discovering host libraries in cross builds.
        env["PKG_CONFIG"] = "false"
        env["VCPKGRS_DISABLE"] = "1"
        command = [
            *maturin,
            "build",
            "--release",
            "--locked",
            "--features",
            "wheel",
            "--target",
            TARGETS[target],
            "--out",
            str(output),
        ]
        if target.startswith("linux-"):
            command += ["--zig", "--compatibility", "manylinux_2_17", "pypi"]
        else:
            command += ["--compatibility", "pypi"]
        if target == "macos-arm64":
            env["MACOSX_DEPLOYMENT_TARGET"] = "11.0"
            if sys.platform != "darwin":
                command.append("--zig")
        print(f"Building {target} (bundled libusb)", flush=True)
        run(command, dry_run=dry_run, env=env)
    run([*maturin, "sdist", "--out", str(output)], dry_run=dry_run)
    print(f"Release artifacts: {output}", flush=True)


def main() -> None:
    """Parse the local build command and report actionable failures."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", nargs="+", choices=[*TARGETS, "all"], required=True)
    parser.add_argument("--dry-run", action="store_true", help="Print build commands without executing")
    parser.add_argument("--dist-dir", type=Path, help="Artifact directory (default: python/dist/VERSION)")
    args = parser.parse_args()
    try:
        release = version()
        output = (args.dist_dir or PYTHON_DIR / "dist" / release).resolve()
        build(args.target, output, dry_run=args.dry_run)
    except (ValueError, OSError) as error:
        parser.exit(1, f"{error}\n")
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from None


if __name__ == "__main__":
    main()
