#!/usr/bin/env bash
set -uo pipefail

# aws_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector AWS Integration test environment

set -x

while getopts a:t:e: flag
do
    case "${flag}" in
        a) ACTION=${OPTARG};;
        t) CONTAINER_TOOL=${OPTARG};;
        e) CONTAINER_ENCLOSURE=${OPTARG};;
        :)
         echo "ERROR: Option -$OPTARG requires an argument"          usage
          ;;
        *)
          echo "ERROR: Invalid option -$OPTARG"
          usage
          ;;
    esac
done
shift $((OPTIND-1))

# Check required switches exist
if [ -z "${ACTION}" ] || [ -z "${CONTAINER_TOOL}" ] || [ -z "${CONTAINER_ENCLOSURE}" ]; then
    usage
fi


ACTION="${action:-"stop"}"
CONTAINER_TOOL="${tool:-"podman"}"
CONTAINER_ENCLOSURE="${enclosure:-"pod"}"

#
# Functions
#

start_podman () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-aws -p 4566:4566 -p 4571:4571 -p 6000:6000 -p 9088:80
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-aws --name vector_ec2_metadata \
  timberiodev/mock-ec2-metadata:latest
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-aws --name vector_localstack_aws \
  -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
  localstack/localstack-full:0.11.6
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-aws --name vector_mockwatchlogs \
  -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-aws -v /var/run:/var/run --name vector_local_ecs \
  -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
}

start_docker () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-aws
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-aws -p 8111:8111 --name vector_ec2_metadata \
  timberiodev/mock-ec2-metadata:latest
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-aws --name vector_localstack_aws \
  -p 4566:4566 -p 4571:4571 \
  -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
  localstack/localstack-full:0.11.6
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-aws -p 6000:6000 --name vector_mockwatchlogs \
  -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-aws -v /var/run:/var/run -p 9088:80 --name vector_local_ecs \
  -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
}

stop_podman () {
	"${CONTAINER_TOOL}" rm --force vector_ec2_metadata vector_mockwatchlogs vector_localstack_aws vector_local_ecs 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-aws 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-aws 2>/dev/null; true
}

stop_docker () {
	"${CONTAINER_TOOL}" rm --force vector_ec2_metadata vector_mockwatchlogs vector_localstack_aws vector_local_ecs 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-aws 2>/dev/null; true
}

echo "Running $ACTION action for AWS integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
