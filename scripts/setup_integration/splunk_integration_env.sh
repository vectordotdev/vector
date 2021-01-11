#!/usr/bin/env bash
set -o pipefail

# splunk_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Splunk Integration test environment

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
  podman pod create --replace --name vector-test-integration-splunk -p 8088:8088 -p 8000:8000 -p 8089:8089
  podman run -d --pod=vector-test-integration-splunk \
    --name splunk timberio/splunk-hec-test:minus_compose
}

start_docker () {
  docker network create vector-test-integration-splunk
  docker run -d --network=vector-test-integration-splunk -p 8088:8088 -p 8000:8000 \
   -p 8089:8089 --name splunk timberio/splunk-hec-test:minus_compose
}

stop_podman () {
  podman rm --force splunk 2>/dev/null; true
  podman pod stop vector-test-integration-splunk 2>/dev/null; true
  podman pod rm --force vector-test-integration-splunk 2>/dev/null; true
}

stop_docker () {
  docker rm --force splunk 2>/dev/null; true
  docker network rm vector-test-integration-splunk 2>/dev/null; true
}

echo "Running $ACTION action for Splunk integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
