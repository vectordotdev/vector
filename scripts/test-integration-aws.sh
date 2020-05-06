#!/usr/bin/env bash
set -euo pipefail

# test-integration-aws.sh
#
# SUMMARY
#
#   Run integration tests for AWS components only.

docker-compose up -d dependencies-aws
cargo test --no-default-features --features aws-integration-tests
