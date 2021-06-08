#!/usr/bin/env bash
set -o pipefail

# dnstap_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Dnstap Integration test environment

if [ $# -ne 1 ]
then
    echo "Usage: $0 {stop|start}" 1>&2; exit 1;
    exit 1
fi
ACTION=$1

#
# Functions
#

SOCKET_DIR="$(pwd)"/tests/data/dnstap/socket

start_podman () {
  podman build -t dnstap_img "$(pwd)"/tests/data/dnstap
  mkdir -p "${SOCKET_DIR}"
  chmod 777 "${SOCKET_DIR}"
  podman run --network host --hostname ns.example.com --name vector_dnstap \
  -v "${SOCKET_DIR}":/bind1/etc/bind/socket \
  -v "${SOCKET_DIR}":/bind2/etc/bind/socket \
  -v "${SOCKET_DIR}":/bind3/etc/bind/socket \
  -d dnstap_img
}

start_docker () {
  docker build -t dnstap_img "$(pwd)"/tests/data/dnstap
  mkdir -p "${SOCKET_DIR}"
  chmod 777 "${SOCKET_DIR}"
  docker run --hostname ns.example.com --name vector_dnstap \
  -v "${SOCKET_DIR}":/bind1/etc/bind/socket \
  -v "${SOCKET_DIR}":/bind2/etc/bind/socket \
  -v "${SOCKET_DIR}":/bind3/etc/bind/socket \
  -d dnstap_img
}

stop_podman () {
  podman rm --force vector_dnstap 2>/dev/null; true
  rm -rf -- "${SOCKET_DIR}"
}

stop_docker () {
  docker rm --force vector_dnstap 2>/dev/null; true
  rm -rf -- "${SOCKET_DIR}"
}

echo "Running $ACTION action for Dnstap integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
