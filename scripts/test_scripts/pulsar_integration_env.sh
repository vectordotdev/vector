#!/usr/bin/env bash
set -uo pipefail

# pulsar_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Pulsar Integration test environment

set -x

while getopts a: flag
do
    case "${flag}" in
        a) action=${OPTARG};;
    esac
done

ACTION="${action:-"stop"}"
CONTAINER_TOOL="${CONTAINER_TOOL:-"podman"}"

case $CONTAINER_TOOL in
  "podman")
    CONTAINER_ENCLOSURE="pod"
    ;;
  "docker")
    CONTAINER_ENCLOSURE="network"
    ;;
  *)
    CONTAINER_ENCLOSURE="unknown"
    ;;
esac

#
# Functions
#

start_podman () {
	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} create --replace --name vector-test-integration-pulsar -p 6650:6650
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-pulsar  --name vector_pulsar \
	 apachepulsar/pulsar bin/pulsar standalone
}

start_docker () {
	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} create vector-test-integration-pulsar
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-pulsar -p 6650:6650 --name vector_pulsar \
	 apachepulsar/pulsar bin/pulsar standalone
}

stop () {
	${CONTAINER_TOOL} rm --force vector_pulsar 2>/dev/null; true
  if [ $CONTAINER_TOOL == "podman" ]
  then
	  ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} stop vector-test-integration-pulsar 2>/dev/null; true
	  ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} rm --force vector-test-integration-pulsar 2>/dev/null; true
  else
	  ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} rm vector-test-integration-pulsar 2>/dev/null; true
fi
}

echo "Running $ACTION action for Pulsar integration tests environment"

$ACTION
