#!/usr/bin/env bash
set -o pipefail

# eventstoredb_metrics_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector EventStoreDB metrics Integration test environment

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
  "${CONTAINER_TOOL}" run -d --name vector_eventstoredb_metric --net=host \
	 --volume "$(pwd)"/tests/data:/etc/vector:ro \
	 eventstore/eventstore --insecure --stats-period-sec=1
}

stop () {
  "${CONTAINER_TOOL}" rm --force vector_eventstoredb_metric 2>/dev/null; true
}

echo "Running $ACTION action for EventStoreDB metric integration tests environment"

${ACTION}
