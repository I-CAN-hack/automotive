#!/usr/bin/env bash
# Publish the built Python release. Pass --dry-run to verify without uploading.
# Other arguments are forwarded to uv publish. Use UV_PUBLISH_TOKEN to authenticate.
set -euo pipefail

python_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd -- "$python_dir"

version="$(uv version --short)"
exec uv publish "dist/$version/*" "$@"
