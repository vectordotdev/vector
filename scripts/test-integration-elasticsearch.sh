#!/usr/bin/env bash

# test-integration-elasticsearch.sh
#
# SUMMARY
#
#   Run integration tests for Elasticsearch components only.

set -euo pipefail

docker-compose up -d dependencies-elasticsearch
cargo test --no-default-features --features es-integration-tests
