#!/usr/bin/env bash
set -euo pipefail

# fmt.sh
#
# SUMMARY
#
#   Applies fmt changes across the repo

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

scripts/check-style.sh --fix
cargo fmt
