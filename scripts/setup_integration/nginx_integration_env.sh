#!/usr/bin/env bash
set -uo pipefail

# nginx_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Nginx Integration test environment

set -x

while getopts a: flag
do
    case "${flag}" in
        a) action=${OPTARG};;
    esac
done

ACTION="${action:-"stop"}"
CONTAINER_TOOL="${CONTAINER_TOOL:-"podman"}"

case $CONTAINER_TOOL in
  "podman")
    CONTAINER_ENCLOSURE="pod"
    ;;
  "docker")
    CONTAINER_ENCLOSURE="network"
    ;;
  *)
    CONTAINER_ENCLOSURE="unknown"
    ;;
esac

#
# Functions
#

start_podman () {
	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} create --replace --name vector-test-integration-nginx -p 8010:8000
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-nginx --name vector_nginx \
	-v $(PWD)tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
}

start_docker () {
	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} create vector-test-integration-nginx
	${CONTAINER_TOOL} run -d --${CONTAINER_ENCLOSURE}=vector-test-integration-nginx -p 8010:8000 --name vector_nginx \
	-v $(PWD)/tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
}

stop () {
	${CONTAINER_TOOL} rm --force vector_nginx 2>/dev/null; true
  if [ $CONTAINER_TOOL == "podman" ]
  then
  	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} stop vector-test-integration-nginx 2>/dev/null; true
  	${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} rm --force vector-test-integration-nginx 2>/dev/null; true
  else
	  ${CONTAINER_TOOL} ${CONTAINER_ENCLOSURE} rm vector-test-integration-nginx 2>/dev/null; true
fi
}

echo "Running $ACTION action for Nginx integration tests environment"

$ACTION
