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

tag="$1"
image="timberiodev/vector-$tag:latest"

#
# Execute
#

docker build \
  -t $image \
  -f scripts/ci-docker-images/$tag/Dockerfile \
  scripts

# Set flags for "docker run".
# Note that the `--privileged` flags is set by default because it is
# required to register `binfmt` handlers, whaich allow to run builders
# for ARM achitectures which need to use `qemu-user`.
docker_flags=("--privileged" "--interactive")
if [ -t 0 ]; then # the script's input is connected to a terminal
  docker_flags+=("--tty")
fi

# pass environment variables prefixed with `PASS_` to the container
# with removed `PASS_` prefix
IFS=$'\n'
for line in $(env | grep '^PASS_' | sed 's/^PASS_//'); do
  docker_flags+=("-e" "$line")
done
unset IFS

docker run \
  "${docker_flags[@]}" \
  -w "$PWD" \
  -v "$PWD":"$PWD" \
  $image \
  "${@:2}"
