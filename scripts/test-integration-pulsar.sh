#!/usr/bin/env bash

# test-integration-pulsar.sh
#
# SUMMARY
#
#   Run integration tests for Pulsar components only.

set -euo pipefail

docker-compose up -d dependencies-pulsar
cargo test --no-default-features --features pulsar-integration-tests
