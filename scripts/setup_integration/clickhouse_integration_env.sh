#!/usr/bin/env bash
set -o pipefail

# clickhouse_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Clickhouse Integration test environment

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
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-clickhouse -p 8123:8123
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-clickhouse --name vector_clickhouse yandex/clickhouse-server:19
}

start_docker () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-clickhouse
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-clickhouse -p 8123:8123 --name vector_clickhouse yandex/clickhouse-server:19
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force vector_clickhouse 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-clickhouse 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-clickhouse 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_clickhouse 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-clickhouse 2>/dev/null; true
}

echo "Running $ACTION action for Clickhouse integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
