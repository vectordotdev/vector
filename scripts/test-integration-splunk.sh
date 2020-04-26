#!/usr/bin/env bash

# test-integration-splunk.sh
#
# SUMMARY
#
#   Run integration tests for Splunk components only.

docker-compose up -d dependencies-splunk
cargo test --no-default-features --features splunk-integration-tests
