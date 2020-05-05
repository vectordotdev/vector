#!/bin/bash
set -euo pipefail

# check-fmt.sh
#
# SUMMARY
#
#   Checks the format of Vector code

cd "$(dirname "${BASH_SOURCE[0]}")/.."
scripts/check-style.sh
cargo fmt -- --check
