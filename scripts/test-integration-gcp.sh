#!/usr/bin/env bash

# test-integration-gcp.sh
#
# SUMMARY
#
#   Run integration tests for GCP components only.

docker-compose up -d dependencies-gcp
cargo test --no-default-features --features gcp-pubsub-integration-tests, gcs-integration-tests
