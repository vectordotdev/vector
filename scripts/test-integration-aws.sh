#!/usr/bin/env bash

# test-integration-aws.sh
#
# SUMMARY
#
#   Run integration tests for AWS components only.

set -euo pipefail

docker-compose up -d dependencies-aws
cargo test --no-default-features --features aws-integration-tests
