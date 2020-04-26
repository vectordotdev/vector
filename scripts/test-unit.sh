#!/usr/bin/env bash

# test-unit.sh
#
# SUMMARY
#
#   Run unit tests

set -euo pipefail

cargo test --no-default-features --target ${TARGET}
