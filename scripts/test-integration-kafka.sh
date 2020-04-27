#!/usr/bin/env bash

# test-integration-kafka.sh
#
# SUMMARY
#
#   Run integration tests for Kafka components only.

set -euo pipefail

docker-compose up -d dependencies-kafka
cargo test --no-default-features --features kafka-integration-tests
