#!/usr/bin/env bash
set -o pipefail

# gcp_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector GCP Integration test environment

# Echo usage if something isn't right.
usage() {
    echo "Usage: $0 [-a Action to run {stop|start} ] [-t The container tool to use {docker|podman} ] [-e The container enclosure to use {pod|network} ]" 1>&2; exit 1;
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
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-gcp -p 8681-8682:8681-8682
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-gcp --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
}

start_docker () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-gcp
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-gcp -p 8681-8682:8681-8682 --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force vector_cloud-pubsub 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-gcp 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-gcp 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_cloud-pubsub 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-gcp 2>/dev/null; true
}

echo "Running $ACTION action for GCP integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
