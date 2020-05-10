#!/usr/bin/env bash
set -euo pipefail

# test-shutdown.sh
#
# SUMMARY
#
#   Run shutdown tests only.

docker-compose up -d dependencies-kafka
cargo test --features shutdown-tests  --test shutdown -- --test-threads 4
