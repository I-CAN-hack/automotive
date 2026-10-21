#!/usr/bin/env bash
# Thin local wrapper; all options and artifact paths go directly to uv publish.
# Usage from python/: ./deploy.sh [--dry-run] 'dist/VERSION/*'
set -euo pipefail

uv publish "$@"
