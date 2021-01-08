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
  "${CONTAINER_TOOL}" pod create --replace --name vector-test-integration-clickhouse -p 8123:8123
  "${CONTAINER_TOOL}" run -d --pod=vector-test-integration-clickhouse --name vector_clickhouse yandex/clickhouse-server:19
}

start_docker () {
  "${CONTAINER_TOOL}" network create vector-test-integration-clickhouse
  "${CONTAINER_TOOL}" run -d --network=vector-test-integration-clickhouse -p 8123:8123 --name vector_clickhouse yandex/clickhouse-server:19
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force vector_clickhouse 2>/dev/null; true
  "${CONTAINER_TOOL}" pod stop vector-test-integration-clickhouse 2>/dev/null; true
  "${CONTAINER_TOOL}" pod rm --force vector-test-integration-clickhouse 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_clickhouse 2>/dev/null; true
  "${CONTAINER_TOOL}" network rm vector-test-integration-clickhouse 2>/dev/null; true
}

echo "Running $ACTION action for Clickhouse integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
