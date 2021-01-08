#!/usr/bin/env bash
set -o pipefail

# nginx_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Nginx Integration test environment

if [ $# -ne 2 ]
then
    echo "Usage: $0 {stop|start} {docker|podman}" 1>&2; exit 1;
    exit 1
fi
ACTION=$1
CONTAINER_TOOL=$2

#
# Functions
#

start_podman () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-nginx -p 8010:8000
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-nginx --name vector_nginx \
	-v "$(pwd)"tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
}

start_docker () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-nginx
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-nginx -p 8010:8000 --name vector_nginx \
	-v "$(pwd)"/tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
}

stop_podman () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-nginx 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-nginx 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_nats 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-nginx 2>/dev/null; true
}

echo "Running $ACTION action for Nginx integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
