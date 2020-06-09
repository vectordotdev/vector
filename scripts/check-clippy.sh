#!/usr/bin/env bash
set -euo pipefail

# check-clippy.sh
#
# SUMMARY
#
#   Checks all Vector code with Clippy

cargo clippy --workspace --all-targets -- -D warnings
