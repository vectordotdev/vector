#!/usr/bin/env bash
set -euo pipefail

# test-integration-influxdb.sh
#
# SUMMARY
#
#   Run integration tests for InfluxDB components only.

docker-compose up -d dependencies-influxdb
cargo test --no-default-features --features influxdb-integration-tests
