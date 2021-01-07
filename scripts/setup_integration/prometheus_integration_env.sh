#!/usr/bin/env bash
set -o pipefail

# prometheus_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Prometheus Integration test environment

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
         echo "ERROR: Option -$OPTARG requires an argument"          usage
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

start () {
  "${CONTAINER_TOOL}" run -d --name vector_prometheus --net=host \
	 --volume "$(pwd)"/tests/data:/etc/vector:ro \
	 prom/prometheus --config.file=/etc/vector/prometheus.yaml
}

stop () {
  "${CONTAINER_TOOL}" rm --force vector_prometheus 2>/dev/null; true
}

echo "Running $ACTION action for Prometheus integration tests environment"

${ACTION}
