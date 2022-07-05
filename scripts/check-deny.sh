#!/usr/bin/env bash
set -euo pipefail

# check-deny.sh
#
# SUMMARY
#
#   Checks the advisories for crate dependencies

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

cargo install --locked cargo-deny
cargo deny --all-features --log-level warn check advisories && cargo deny --all-features --log-level warn check licenses
