#!/usr/bin/env bash

# test-integration-docker.sh
#
# SUMMARY
#
#   Run integration tests for Docker components only.

set -euo pipefail

cargo test --no-default-features --features docker-integration-tests
