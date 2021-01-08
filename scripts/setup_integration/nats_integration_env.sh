#!/usr/bin/env bash
set -o pipefail

# nats_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector NATS Integration test environment

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
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-nats -p 4222:4222
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-nats  --name vector_nats \
	 nats
}

start_docker () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-nats
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-nats -p 4222:4222 --name vector_nats \
	 nats
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force vector_nats 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-nats 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-nats 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_nats 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-nats 2>/dev/null; true
}

echo "Running $ACTION action for NATS integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
