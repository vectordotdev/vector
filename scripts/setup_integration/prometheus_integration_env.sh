#!/usr/bin/env bash
set -o pipefail

# prometheus_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Prometheus Integration test environment

if [ $# -ne 1 ]
then
  echo "Usage: $0 {stop|start}" 1>&2; exit 1;
  exit 1
fi
ACTION=$1

#
# Functions
#

start () {
  "${CONTAINER_TOOL}" run -d --name vector_prometheus --net=host \
	 --volume "$(pwd)"/tests/data:/etc/vector:ro \
	 prom/prometheus:v2.31.0 --config.file=/etc/vector/prometheus.yaml
}

stop () {
  "${CONTAINER_TOOL}" rm --force vector_prometheus 2>/dev/null; true
}

echo "Running $ACTION action for Prometheus integration tests environment"

${ACTION}
