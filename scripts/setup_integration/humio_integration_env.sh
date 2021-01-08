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
  "${CONTAINER_TOOL}" pod create --replace --name vector-test-integration-humio -p 8080:8080
  "${CONTAINER_TOOL}" run -d --pod=vector-test-integration-humio --name vector_humio humio/humio:1.13.1
}

start_docker () {
   "${CONTAINER_TOOL}" network create vector-test-integration-humio
  "${CONTAINER_TOOL}" run -d --network=vector-test-integration-humio -p 8080:8080 --name vector_humio humio/humio:1.13.1
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force vector_humio 2>/dev/null; true
  "${CONTAINER_TOOL}" pod stop vector-test-integration-humio 2>/dev/null; true
  "${CONTAINER_TOOL}" pod rm --force vector-test-integration-humio 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_humio 2>/dev/null; true
  "${CONTAINER_TOOL}" rm --force vector_humio 2>/dev/null; true
  "${CONTAINER_TOOL}" network rm vector-test-integration-humio 2>/dev/null; true
}

echo "Running $ACTION action for Humio integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
