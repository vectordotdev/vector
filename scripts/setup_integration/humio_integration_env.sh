#!/usr/bin/env bash
set -o pipefail

# humio_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Humio Integration test environment

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
  podman pod create --replace --name vector-test-integration-humio -p 8080:8080
  podman run -d --pod=vector-test-integration-humio --name vector_humio humio/humio:1.13.1
}

start_docker () {
  docker network create vector-test-integration-humio
  docker run -d --network=vector-test-integration-humio -p 8080:8080 --name vector_humio humio/humio:1.13.1
}

stop_podman () {
  podman rm --force vector_humio 2>/dev/null; true
  podman pod stop vector-test-integration-humio 2>/dev/null; true
  podman pod rm --force vector-test-integration-humio 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_humio 2>/dev/null; true
  docker network rm vector-test-integration-humio 2>/dev/null; true
}

echo "Running $ACTION action for Humio integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
