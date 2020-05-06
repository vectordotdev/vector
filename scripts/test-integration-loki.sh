#!/usr/bin/env bash
set -euo pipefail

# test-integration-loki.sh
#
# SUMMARY
#
#   Run integration tests for Loki components only.

docker-compose up -d dependencies-loki
cargo test --no-default-features --features loki-integration-tests
