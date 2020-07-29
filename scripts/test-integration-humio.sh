#!/usr/bin/env bash
set -euo pipefail

# test-integration-humio.sh
#
# SUMMARY
#
#   Run integration tests for Humio components only.

docker-compose up -d dependencies-humio
cargo test --no-default-features --features humio-integration-tests
