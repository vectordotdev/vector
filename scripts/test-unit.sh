#!/usr/bin/env bash

# test-unit.sh
#
# SUMMARY
#
#   Run unit tests

set -euo pipefail

if [ -z "${TARGET:-}" ]; then
    cargo test --all --no-default-features
else 
    cargo test --all --no-default-features --target "${TARGET}"
fi
