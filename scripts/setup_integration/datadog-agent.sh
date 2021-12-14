#!/usr/bin/env bash
set -o pipefail

# redis_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Redis Integration test environment

if [ $# -ne 1 ]
then
    echo "Usage: $0 {stop|start}" 1>&2; exit 1;
    exit 1
fi
ACTION=$1

#
# Functions
#

start_podman () {
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml up -d
}

start_docker () {
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml up -d
}

stop_podman () {
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml down
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml rm -f
}

stop_docker () {
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml down
  docker-compose -f scripts/setup_integration/docker-compose.datadog-agent.yml rm -f
}

echo "Running $ACTION action for Redis integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
