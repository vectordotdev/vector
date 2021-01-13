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
  podman pod create --replace --name vector-test-integration-nginx -p 8010:8000
  podman run -d --pod=vector-test-integration-nginx --name vector_nginx \
	-v "$(pwd)"tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
}

start_docker () {
  docker network create vector-test-integration-nginx
  docker run -d --network=vector-test-integration-nginx -p 8010:8000 --name vector_nginx \
	-v "$(pwd)"/tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
}

stop_podman () {
  podman rm --force vector_nginx 2>/dev/null; true
  podman pod stop vector-test-integration-nginx 2>/dev/null; true
  podman pod rm --force vector-test-integration-nginx 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_nginx 2>/dev/null; true
  docker network rm vector-test-integration-nginx 2>/dev/null; true
}

echo "Running $ACTION action for Nginx integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
