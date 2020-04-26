#!/usr/bin/env bash

# test-integration-clickhouse.sh
#
# SUMMARY
#
#   Run integration tests for Clickhouse components only.

set -euo pipefail

docker-compose up -d dependencies-clickhouse
cargo test --no-default-features --features clickhouse-integration-tests
