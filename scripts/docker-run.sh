#!/usr/bin/env bash
set -euo pipefail

# docker-run.sh
#
# SUMMARY
#
#   Builds given `scripts/ci-docker-images/*` and runs a command inside of
#   the provided container based on this image.

#
# Requirements
#

if [ -z "${1:-}" ]; then
  echo "You must pass the docker image tag as the first argument"
  exit 1
fi

if [ -z "${2:-}" ]; then
  echo "You must pass a command to execute as the second argument"
  exit 1
fi

#
# Variables
#

DOCKER="${USE_CONTAINER:-"docker"}"
TAG="$1"
IMAGE="timberiodev/vector-$TAG:latest"

#
# (Re)Build
#
if ! $DOCKER inspect "$IMAGE" >/dev/null 2>&1 || [ "${REBUILD_CONTAINER_IMAGE:-"true"}" == "true" ]; then
  $DOCKER build \
    --file "scripts/ci-docker-images/$TAG/Dockerfile" \
    --tag "$IMAGE" \
    .
fi

#
# Execute
#

# Set flags for "docker run".
# The `--rm` flag is used to delete containers on exit.
# The `--interactive` flag is used to keep `stdin` open.
DOCKER_FLAGS=("--rm" "--interactive")
# If the script's input is connected to a terminal, then
# use `--tty` to allocate a pseudo-TTY.
if [ -t 0 ]; then
  DOCKER_FLAGS+=("--tty")
fi
# If `DOCKER_PRIVILEGED` environment variable is set to true,
# pass `--privileged`. One use case is to register `binfmt`
# handlers in order to run builders for ARM architectures
# using `qemu-user`.
if [ "${DOCKER_PRIVILEGED:-"false"}" == "true" ]; then
  DOCKER_FLAGS+=("--privileged")
fi

# pass environment variables prefixed with `PASS_` to the container
# with removed `PASS_` prefix
IFS=$'\n'
for LINE in $(env | grep '^PASS_' | sed 's/^PASS_//'); do
  DOCKER_FLAGS+=("-e" "$LINE")
done
unset IFS

$DOCKER run \
  "${DOCKER_FLAGS[@]}" \
  -w "$PWD" \
  -v "$PWD":"$PWD" \
  "$IMAGE" \
  "${@:2}"
