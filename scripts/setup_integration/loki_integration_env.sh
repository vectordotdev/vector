#!/usr/bin/env bash
set -o pipefail

# loki_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Loki Integration test environment

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
  podman pod create --replace --name vector-test-integration-loki -p 3100:3100
  podman run -d --pod=vector-test-integration-loki -v "$(pwd)"/tests/data:/etc/loki \
	 --name vector_loki grafana/loki:2.1.0 -config.file=/etc/loki/loki-config.yaml
}

start_docker () {
  docker network create vector-test-integration-loki
  docker run -d --network=vector-test-integration-loki -p 3100:3100 -v "$(pwd)"/tests/data:/etc/loki \
	 --name vector_loki grafana/loki:2.1.0 -config.file=/etc/loki/loki-config.yaml
}

stop_podman () {
  podman rm --force vector_loki 2>/dev/null; true
  podman pod stop vector-test-integration-loki 2>/dev/null; true
  podman pod rm --force vector-test-integration-loki 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_loki 2>/dev/null; true
  docker network rm vector-test-integration-loki 2>/dev/null; true
}

echo "Running $ACTION action for Loki integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
