#!/usr/bin/env bash
set -uo pipefail

# nats_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector NATS Integration test environment

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
	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} create --replace --name vector-test-integration-nats -p 4222:4222
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-nats  --name vector_nats \
	 nats
}

start_docker () {
	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} create vector-test-integration-nats
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-nats -p 4222:4222 --name vector_nats \
	 nats
}

stop () {
	${CONTAINER_TOOL} rm --force vector_nats 2>/dev/null; true
  if [ $CONTAINER_TOOL == "podman" ]
  then
	  ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} stop vector-test-integration-nats 2>/dev/null; true
	  ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} rm --force vector-test-integration-nats 2>/dev/null; true
  else
	  ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} rm vector-test-integration-nats 2>/dev/null; true
fi
}

echo "Running $ACTION action for NATS integration tests environment"

$ACTION
