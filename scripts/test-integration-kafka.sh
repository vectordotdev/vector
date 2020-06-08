#!/usr/bin/env bash
set -euo pipefail

# test-integration-kafka.sh
#
# SUMMARY
#
#   Run integration tests for Kafka components only.

docker-compose up -d dependencies-kafka
cargo test --no-default-features --features kafka-integration-tests,rdkafka-plain
