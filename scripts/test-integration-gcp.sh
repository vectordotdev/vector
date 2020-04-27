#!/usr/bin/env bash

# test-integration-gcp.sh
#
# SUMMARY
#
#   Run integration tests for GCP components only.

set -euo pipefail

docker-compose up -d dependencies-gcp
cargo test --no-default-features --features gcp-integration-tests
