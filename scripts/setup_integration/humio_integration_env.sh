#!/usr/bin/env bash
set -o pipefail

# humio_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Humio Integration test environment

if [ $# -ne 2 ]
then
    echo "Usage: $0 {stop|start} {docker|podman}" 1>&2; exit 1;
    exit 1
fi
ACTION=$1
CONTAINER_TOOL=$2
#
# Functions
#

start_podman () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-humio -p 8080:8080
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-humio --name vector_humio humio/humio:1.13.1
}

start_docker () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-humio
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-humio -p 8080:8080 --name vector_humio humio/humio:1.13.1
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force vector_humio 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-humio 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-humio 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_humio 2>/dev/null; true
  "${CONTAINER_TOOL}" rm --force vector_humio 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-humio 2>/dev/null; true
}

echo "Running $ACTION action for Humio integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
