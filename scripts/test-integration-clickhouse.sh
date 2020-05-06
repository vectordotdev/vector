#!/usr/bin/env bash
set -euo pipefail

# test-integration-clickhouse.sh
#
# SUMMARY
#
#   Run integration tests for Clickhouse components only.

docker-compose up -d dependencies-clickhouse
cargo test --no-default-features --features clickhouse-integration-tests
