#!/usr/bin/env bash
set -euo pipefail

# start-docker-registry.sh
#
# SUMMARY
#
#   Starts a Docker Distribution instance in docker and prints it's IP address
#   to the stdout.
#
#   Useful for conducting tests involving Vector docker images.
#

CONTAINER_NAME="${1:-"vector-docker-registry"}"
PORT="${2:-"5000"}"

IS_ALREADY_RUNNING="$(docker inspect -f '{{.State.Running}}' "$CONTAINER_NAME" 2>/dev/null || true)"
if [ "${IS_ALREADY_RUNNING}" != 'true' ]; then
  docker run \
    -d \
    --restart=always \
    -p "$PORT:5000" \
    --name "$CONTAINER_NAME" \
    registry:2 > /dev/null
fi

IP_ADDRESS="$(docker inspect -f '{{.NetworkSettings.IPAddress}}' "$CONTAINER_NAME")"
echo "$IP_ADDRESS:$PORT"
