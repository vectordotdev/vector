#!/usr/bin/env bash
set -euo pipefail

# test-shutdown.sh
#
# SUMMARY
#
#   Run shutdown tests only.

docker-compose up -d dependencies-kafka
cargo test --no-default-features --features shutdown-tests
