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
  podman pod create --replace --name vector-test-integration-redis -p 6379:6379
  podman run -d --pod=vector-test-integration-redis  --name vector_redis \
	 redis
}

start_docker () {
  docker-compose -f scripts/setup_integration/docker-compose.redis.yml up -d
}

stop_podman () {
  podman rm --force vector_redis 2>/dev/null; true
  podman pod stop vector-test-integration-redis 2>/dev/null; true
  podman pod rm --force vector-test-integration-redis 2>/dev/null; true
}

stop_docker () {
  docker-compose -f scripts/setup_integration/docker-compose.redis.yml stop
  docker-compose -f scripts/setup_integration/docker-compose.redis.yml rm -f
}

echo "Running $ACTION action for Redis integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
