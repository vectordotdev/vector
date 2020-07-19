#!/usr/bin/env bash
set -euo pipefail

# test-cli.sh
#
# SUMMARY
#
#   Run cli tests only.

cargo test --test cli --features cli-tests -- --test-threads 4
