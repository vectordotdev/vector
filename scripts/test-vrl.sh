#!/bin/bash
set -euo pipefail

# test-vrl.sh
#
# SUMMARY
#
#   Run the Vector Remap Language test suite

(
  cd "$(dirname "${BASH_SOURCE[0]}")/../lib/vrl/tests"

  cargo run
)

