#!/bin/bash
set -euo pipefail

# check-code.sh
#
# SUMMARY
#
#   Checks all Vector code

cargo check --workspace --all-targets
