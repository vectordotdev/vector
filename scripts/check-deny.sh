#!/usr/bin/env bash
set -euo pipefail

# check-deny.sh
#
# SUMMARY
#
#   Checks the advisories licenses and sources for crate dependencies

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

cargo install --locked cargo-deny
cargo deny --all-features check all
