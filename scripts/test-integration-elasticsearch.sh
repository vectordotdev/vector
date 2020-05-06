#!/usr/bin/env bash
set -euo pipefail

# test-integration-elasticsearch.sh
#
# SUMMARY
#
#   Run integration tests for Elasticsearch components only.

docker-compose up -d dependencies-elasticsearch
cargo test --no-default-features --features es-integration-tests
