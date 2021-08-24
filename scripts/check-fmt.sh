#!/usr/bin/env bash
set -euo pipefail

# check-fmt.sh
#
# SUMMARY
#
#   Checks the format of Vector code

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

cargo fmt -- --check
