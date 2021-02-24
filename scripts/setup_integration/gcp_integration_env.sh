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
  podman pod create --replace --name vector-test-integration-gcp -p 8681-8682:8681-8682
  podman run -d --pod=vector-test-integration-gcp --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
}

start_docker () {
  docker network create vector-test-integration-gcp
  docker run -d --network=vector-test-integration-gcp -p 8681-8682:8681-8682 --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
}

stop_podman () {
  podman rm --force vector_cloud-pubsub 2>/dev/null; true
  podman pod stop vector-test-integration-gcp 2>/dev/null; true
  podman pod rm --force vector-test-integration-gcp 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_cloud-pubsub 2>/dev/null; true
  docker network rm vector-test-integration-gcp 2>/dev/null; true
}

echo "Running $ACTION action for GCP integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
