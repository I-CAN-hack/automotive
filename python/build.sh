#!/usr/bin/env bash
# Run inside nix develop. Build all release wheels unless --target is specified.
set -euo pipefail

python_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd -- "$python_dir"

exec uv run --locked --group release python build_wheels.py "$@"
