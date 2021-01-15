#!/usr/bin/env bash
set -o pipefail

# pulsar_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Pulsar Integration test environment

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
  podman pod create --replace --name vector-test-integration-pulsar -p 6650:6650
  podman run -d --pod=vector-test-integration-pulsar  --name vector_pulsar \
	 apachepulsar/pulsar bin/pulsar standalone
}

start_docker () {
  docker network create vector-test-integration-pulsar
  docker run -d --network=vector-test-integration-pulsar -p 6650:6650 --name vector_pulsar \
	 apachepulsar/pulsar bin/pulsar standalone
}

stop_podman () {
  podman rm --force vector_pulsar 2>/dev/null; true
  podman pod stop vector-test-integration-pulsar 2>/dev/null; true
  podman pod rm --force vector-test-integration-pulsar 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_pulsar 2>/dev/null; true
  docker network rm vector-test-integration-pulsar 2>/dev/null; true
}

echo "Running $ACTION action for Pulsar integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
