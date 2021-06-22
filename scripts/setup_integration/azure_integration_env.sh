#!/usr/bin/env bash
set -o pipefail

# azure_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Azure Integration test environment

set -x

if [ $# -ne 1 ]; then
  echo "Usage: $0 {stop|start}" 1>&2
  exit 1
  exit 1
fi
ACTION=$1

#
# Functions
#

start_podman() {
  podman pod create --replace --name vector-test-integration-azure -p 10000-10001:10000-10001
  podman run -d --pod=vector-test-integration-azure --name vector_local_azure_blob \
    mcr.microsoft.com/azure-storage/azurite:3.11.0 azurite --blobHost 0.0.0.0 --loose
}

start_docker() {
  docker network create vector-test-integration-azure
  docker run -d --network=vector-test-integration-azure -v /var/run:/var/run -p 10000-10001:10000-10001 --name vector_local_azure_blob \
    mcr.microsoft.com/azure-storage/azurite:3.11.0 azurite --blobHost 0.0.0.0 --loose
}

stop_podman() {
  podman rm --force vector_local_azure_blob 2>/dev/null; true
  podman pod stop vector-test-integration-azure 2>/dev/null; true
  podman pod rm --force vector-test-integration-azure 2>/dev/null; true
}

stop_docker() {
  docker rm --force vector_local_azure_blob 2>/dev/null; true
  docker network rm vector-test-integration-azure 2>/dev/null; true
}

echo "Running $ACTION action for Azure integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
