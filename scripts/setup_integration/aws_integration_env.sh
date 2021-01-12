#!/usr/bin/env bash
set -o pipefail

# aws_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector AWS Integration test environment

set -x

if [ $# -ne 1 ]
then
  echo "Usage: $0 {stop|start}" 1>&2; exit 1;
  exit 1
fi
ACTION=$1

#
# Functions
#

start_podman () {
  podman pod create --replace --name vector-test-integration-aws -p 4566:4566 -p 4571:4571 -p 6000:6000 -p 9088:80
  podman run -d --pod=vector-test-integration-aws --name vector_ec2_metadata \
  timberiodev/mock-ec2-metadata:latest
  podman run -d --pod=vector-test-integration-aws --name vector_localstack_aws \
  -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
  localstack/localstack-full:0.11.6
  podman run -d --pod=vector-test-integration-aws --name vector_mockwatchlogs \
  -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
  podman run -d --pod=vector-test-integration-aws -v /var/run:/var/run --name vector_local_ecs \
  -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
}

start_docker () {
  docker network create vector-test-integration-aws
  docker run -d --network=vector-test-integration-aws -p 8111:8111 --name vector_ec2_metadata \
  timberiodev/mock-ec2-metadata:latest
  docker run -d --network=vector-test-integration-aws --name vector_localstack_aws \
  -p 4566:4566 -p 4571:4571 \
  -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
  localstack/localstack-full:0.11.6
  docker run -d --network=vector-test-integration-aws -p 6000:6000 --name vector_mockwatchlogs \
  -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
  docker run -d --network=vector-test-integration-aws -v /var/run:/var/run -p 9088:80 --name vector_local_ecs \
  -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
}

stop_podman () {
  podman rm --force vector_ec2_metadata vector_mockwatchlogs vector_localstack_aws vector_local_ecs 2>/dev/null; true
  podman pod stop vector-test-integration-aws 2>/dev/null; true
  podman pod rm --force vector-test-integration-aws 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_ec2_metadata vector_mockwatchlogs vector_localstack_aws vector_local_ecs 2>/dev/null; true
  docker network rm vector-test-integration-aws 2>/dev/null; true
}

echo "Running $ACTION action for AWS integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
