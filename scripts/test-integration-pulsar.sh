#!/usr/bin/env bash
set -euo pipefail

# test-integration-pulsar.sh
#
# SUMMARY
#
#   Run integration tests for Pulsar components only.

docker-compose up -d dependencies-pulsar
cargo test --no-default-features --features pulsar-integration-tests
