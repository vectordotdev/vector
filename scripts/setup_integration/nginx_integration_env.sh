#!/usr/bin/env bash
set -o pipefail

# nginx_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Nginx Integration test environment

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
  "${CONTAINER_TOOL}" pod create --replace --name vector-test-integration-nginx -p 8010:8000
  "${CONTAINER_TOOL}" run -d --pod=vector-test-integration-nginx --name vector_nginx \
	-v "$(pwd)"tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
}

start_docker () {
  "${CONTAINER_TOOL}" network create vector-test-integration-nginx
  "${CONTAINER_TOOL}" run -d --network=vector-test-integration-nginx -p 8010:8000 --name vector_nginx \
	-v "$(pwd)"/tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
}

stop_podman () {
  "${CONTAINER_TOOL}" pod stop vector-test-integration-nginx 2>/dev/null; true
  "${CONTAINER_TOOL}" pod rm --force vector-test-integration-nginx 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_nats 2>/dev/null; true
  "${CONTAINER_TOOL}" network rm vector-test-integration-nginx 2>/dev/null; true
}

echo "Running $ACTION action for Nginx integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
