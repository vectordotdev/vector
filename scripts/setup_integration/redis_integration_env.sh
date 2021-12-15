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

start () {
  docker-compose -f scripts/setup_integration/docker-compose.redis.yml up -d
}

stop () {
  docker-compose -f scripts/setup_integration/docker-compose.redis.yml stop
  docker-compose -f scripts/setup_integration/docker-compose.redis.yml rm -f
}

echo "Running $ACTION action for Redis integration tests environment"

"${ACTION}"
