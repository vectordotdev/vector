#!/usr/bin/env bash
set -o pipefail

# clickhouse_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Clickhouse Integration test environment

# Echo usage if something isn't right.
usage() {
    echo "Usage: $0 [-a Action to run {stop|start} ] [-t The container tool to use {docker|pdoman} ]  [-t The container enclosure to use {pod|network} ]" 1>&2; exit 1;
}

while getopts a:t:e: flag
do
    case "${flag}" in
        a) ACTION=${OPTARG};;
        t) CONTAINER_TOOL=${OPTARG};;
        e) CONTAINER_ENCLOSURE=${OPTARG};;
        :)
        echo "ERROR: Option -$OPTARG requires an argument"
        usage
        ;;
        *)
        echo "ERROR: Invalid option -$OPTARG"
        usage
        ;;
    esac
done
shift $((OPTIND-1))
# Check required switches exist
if [ -z "${ACTION}" ] || [ -z "${CONTAINER_TOOL}" ] || [ -z "${CONTAINER_ENCLOSURE}" ]; then
    usage
fi

ACTION="${ACTION:-"stop"}"
CONTAINER_TOOL="${CONTAINER_TOOL:-"podman"}"
CONTAINER_ENCLOSURE="${CONTAINER_ENCLOSURE:-"pod"}"

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
