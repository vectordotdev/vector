#!/bin/bash
set -euo pipefail

# fmt.sh
#
# SUMMARY
#
#   Applies fmt changes across the repo

cd "$(dirname "${BASH_SOURCE[0]}")/.."
scripts/check-style.sh --fix
cargo fmt
