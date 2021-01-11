#!/usr/bin/env bash
set -o pipefail

# clickhouse_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Clickhouse Integration test environment

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
  podman pod create --replace --name vector-test-integration-clickhouse -p 8123:8123
  podman run -d --pod=vector-test-integration-clickhouse --name vector_clickhouse yandex/clickhouse-server:19
}

start_docker () {
  docker network create vector-test-integration-clickhouse
  docker run -d --network=vector-test-integration-clickhouse -p 8123:8123 --name vector_clickhouse yandex/clickhouse-server:19
}

stop_podman () {
  podman rm --force vector_clickhouse 2>/dev/null; true
  podman pod stop vector-test-integration-clickhouse 2>/dev/null; true
  podman pod rm --force vector-test-integration-clickhouse 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_clickhouse 2>/dev/null; true
  docker network rm vector-test-integration-clickhouse 2>/dev/null; true
}

echo "Running $ACTION action for Clickhouse integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
