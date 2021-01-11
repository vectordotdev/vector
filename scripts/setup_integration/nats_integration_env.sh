#!/usr/bin/env bash
set -o pipefail

# nats_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector NATS Integration test environment

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
  podman pod create --replace --name vector-test-integration-nats -p 4222:4222
  podman run -d --pod=vector-test-integration-nats  --name vector_nats \
	 nats
}

start_docker () {
  docker network create vector-test-integration-nats
  docker run -d --network=vector-test-integration-nats -p 4222:4222 --name vector_nats \
	 nats
}

stop_podman () {
  podman rm --force vector_nats 2>/dev/null; true
  podman pod stop vector-test-integration-nats 2>/dev/null; true
  podman pod rm --force vector-test-integration-nats 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_nats 2>/dev/null; true
  docker network rm vector-test-integration-nats 2>/dev/null; true
}

echo "Running $ACTION action for NATS integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
