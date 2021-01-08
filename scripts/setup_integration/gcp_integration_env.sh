#!/usr/bin/env bash
set -o pipefail

# gcp_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector GCP Integration test environment

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
  "${CONTAINER_TOOL}" pod create --replace --name vector-test-integration-gcp -p 8681-8682:8681-8682
  "${CONTAINER_TOOL}" run -d --pod=vector-test-integration-gcp --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
}

start_docker () {
   "${CONTAINER_TOOL}" network create vector-test-integration-gcp
  "${CONTAINER_TOOL}" run -d --network=vector-test-integration-gcp -p 8681-8682:8681-8682 --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force vector_cloud-pubsub 2>/dev/null; true
  "${CONTAINER_TOOL}" pod stop vector-test-integration-gcp 2>/dev/null; true
  "${CONTAINER_TOOL}" pod rm --force vector-test-integration-gcp 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_cloud-pubsub 2>/dev/null; true
  "${CONTAINER_TOOL}" network rm vector-test-integration-gcp 2>/dev/null; true
}

echo "Running $ACTION action for GCP integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
