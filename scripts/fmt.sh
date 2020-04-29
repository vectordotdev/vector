#!/bin/bash
set -eo pipefail

# fmt.sh
#
# SUMMARY
#
#   Applies fmt changes across the repo

check-style.sh --fix
cargo fmt