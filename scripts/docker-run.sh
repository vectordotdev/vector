#!/usr/bin/env bash

# run-docker.sh
#
# SUMMARY
#
#   Builds given CI Docker image and runs a command inside of a container
#   based on this image.

set -eou pipefail

tag="$1"
image="timberiodev/vector-$tag:latest"

docker build \
  -t $image \
  -f scripts/ci-docker-images/$tag/Dockerfile \
  scripts/ci-docker-images

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
