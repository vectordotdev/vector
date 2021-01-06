#!/usr/bin/env bash
set -uo pipefail

# setup_integration_env.sh
#
# SUMMARY
#
#  Sets up Vector integration test environments

set -x

while getopts i:a:t: flag
do
    case "${flag}" in
        i) integration=${OPTARG};;
        a) action=${OPTARG};;
        t) tool=${OPTARG};;
    esac
done

INTEGRATION="${integration:-"none"}"
ACTION="${action:-"stop"}"
CONTAINER_TOOL="${CONTAINER_TOOL:-"podman"}"

case $CONTAINER_TOOL in
  "podman")
    CONTAINER_ENCLOSURE="pod"
    ;;
  "docker")
    CONTAINER_ENCLOSURE="network"
    ;;
  *)
    CONTAINER_ENCLOSURE="unknown"
    ;;
esac

#
# Functions
#

start_podman () {
	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} create --replace --name vector-test-integration-aws -p 4566:4566 -p 4571:4571 -p 6000:6000 -p 9088:80
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-aws --name vector_ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-aws --name vector_localstack_aws \
	 -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
	 localstack/localstack-full:0.11.6
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-aws --name vector_mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-aws -v /var/run:/var/run --name vector_local_ecs \
	 -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
}

start_docker () {
	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} create vector-test-integration-aws
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-aws -p 8111:8111 --name vector_ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-aws --name vector_localstack_aws \
	 -p 4566:4566 -p 4571:4571 \
	 -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
	 localstack/localstack-full:0.11.6
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-aws -p 6000:6000 --name vector_mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-aws -v /var/run:/var/run -p 9088:80 --name vector_local_ecs \
	 -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
}

stop () {
	${CONTAINER_TOOL} rm --force vector_ec2_metadata vector_mockwatchlogs vector_localstack_aws vector_local_ecs 2>/dev/null; true
  if [ $CONTAINER_TOOL == "podman" ]
  then
    ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} stop vector-test-integration-aws 2>/dev/null; true
    ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} rm --force vector-test-integration-aws 2>/dev/null; true
  else
    ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} rm vector-test-integration-aws 2>/dev/null; true
  fi
}

echo "Running $ACTION action for AWS integration tests environment"

$ACTION
