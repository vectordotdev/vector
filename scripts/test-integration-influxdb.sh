#!/usr/bin/env bash

# test-integration-influxdb.sh
#
# SUMMARY
#
#   Run integration tests for InfluxDB components only.

set -euo pipefail

docker-compose up -d dependencies-influxdb
cargo test --no-default-features --features influxdb-integration-tests
