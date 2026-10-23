#!/usr/bin/env bash
# Publish the Rust library. Pass --dry-run to verify without uploading.
# Other arguments are forwarded to cargo publish (for example --allow-dirty).
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cargo publish --manifest-path "$repo_dir/Cargo.toml" --package automotive \
    --registry crates-io --features all --locked "$@"
