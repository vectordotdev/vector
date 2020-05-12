#!/bin/bash
set -euo pipefail

# check-code.sh
#
# SUMMARY
#
#   Checks all Vector code

export RUSTFLAGS="${RUSTFLAGS:-"-D warnings"}"
cargo check --workspace --all-targets
