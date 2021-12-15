#!/usr/bin/env bash
set -o pipefail

# datadog-agent.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Datadog Agent Integration test environment

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
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml up -d
}

stop () {
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml down
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml rm -f
}

echo "Running $ACTION action for Datadog agent integration tests environment"

"${ACTION}"
