#!/usr/bin/env bash

# docker-run.sh
#
# SUMMARY
#
#   Builds given `scripts/ci-docker-images/*` and runs a command inside of
#   the provided container based on this image.

set -eou pipefail

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

DOCKER=${USE_CONTAINER:-docker}
tag="$1"
image="timberiodev/vector-$tag:latest"

#
# (Re)Build
#
if ! $DOCKER inspect $image >/dev/null 2>&1 || [ "${REBUILD_CONTAINER_IMAGE:-true}" == true ]
then
  $DOCKER build \
    --file scripts/ci-docker-images/$tag/Dockerfile \
    --tag $image \
    .
fi

#
# Execute
#

# Set flags for "docker run".
# The `--rm` flag is used to delete containers on exit.
# The `--interactive` flag is used to keep `stdin` open.
docker_flags=("--rm" "--interactive")
# If the script's input is connected to a terminal, then
# use `--tty` to allocate a pseudo-TTY.
if [ -t 0 ]; then
  docker_flags+=("--tty")
fi
# If `DOCKER_PRIVILEGED` environment variable is set to true,
# pass `--privileged`. One use case is to register `binfmt`
# handlers in order to run builders for ARM architectures
# using `qemu-user`.
if [ "${DOCKER_PRIVILEGED:-false}" == true ]; then
  docker_flags+=("--privileged")
fi

# pass environment variables prefixed with `PASS_` to the container
# with removed `PASS_` prefix
IFS=$'\n'
for line in $(env | grep '^PASS_' | sed 's/^PASS_//'); do
  docker_flags+=("-e" "$line")
done
unset IFS

$DOCKER run \
  "${docker_flags[@]}" \
  -w "$PWD" \
  -v "$PWD":"$PWD" \
  $image \
  "${@:2}"
