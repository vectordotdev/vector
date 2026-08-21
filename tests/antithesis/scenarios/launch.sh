#!/usr/bin/env bash
# Compatibility entry point for launching from the scenarios directory.
set -euo pipefail

ANTITHESIS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$ANTITHESIS_DIR/scripts/launch.sh" "$@"
