#!/usr/bin/env bash
set -uo pipefail

# prometheus_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Prometheus Integration test environment

set -x

while getopts a:t:e: flag
do
    case "${flag}" in
        a) action=${OPTARG};;
        t) tool=${OPTARG};;
        e) enclosure=${OPTARG};;

    esac
done

ACTION="${action:-"stop"}"
CONTAINER_TOOL="${tool:-"podman"}"
CONTAINER_ENCLOSURE="${enclosure:-"pod"}"

#
# Functions
#

start () {
	${CONTAINER_TOOL} run -d --name vector_prometheus --net=host \
	 --volume $(PWD)/tests/data:/etc/vector:ro \
	 prom/prometheus --config.file=/etc/vector/prometheus.yaml
}

stop () {
	${CONTAINER_TOOL} rm --force vector_prometheus 2>/dev/null; true
}

echo "Running $ACTION action for Prometheus integration tests environment"

${ACTION}
