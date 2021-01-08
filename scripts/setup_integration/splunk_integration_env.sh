#!/usr/bin/env bash
set -o pipefail

# splunk_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Splunk Integration test environment

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
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-splunk -p 8088:8088 -p 8000:8000 -p 8089:8089
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-splunk \
     --name splunk timberio/splunk-hec-test:minus_compose
}

start_docker () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-splunk
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-splunk -p 8088:8088 -p 8000:8000 \
   -p 8089:8089 --name splunk timberio/splunk-hec-test:minus_compose
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force splunk 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-splunk 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-splunk 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force splunk 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-splunk 2>/dev/null; true
}

echo "Running $ACTION action for Splunk integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
