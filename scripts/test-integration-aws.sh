#!/usr/bin/env bash

# test-integration-aws.sh
#
# SUMMARY
#
#   Run integration tests for AWS components only.

set -euo pipefail

docker-compose up -d dependencies-aws
cargo test --no-default-features --features cloudwatch-logs-integration-tests,cloudwatch-metrics-integration-tests,ec2-metadata-integration-tests,firehose-integration-tests,kinesis-integration-tests,s3-integration-tests
