#!/usr/bin/env bash

# test-integration-loki.sh
#
# SUMMARY
#
#   Run integration tests for Loki components only.

set -euo pipefail

docker-compose up -d dependencies-loki
cargo test --no-default-features --features loki-integration-tests
