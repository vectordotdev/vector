#!/usr/bin/env bash
set -euo pipefail

# test-unit.sh
#
# SUMMARY
#
#   Run unit tests

if [ -z "${TARGET:-}" ]; then
  cargo test --workspace --no-default-features
else
  cargo test --workspace --no-default-features --target "${TARGET}"
fi
