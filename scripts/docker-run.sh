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

# set flags for "docker run"
docker_flags=("--privileged" "--interactive")
if [ -t 0 ]; then # the script's input is connected to a terminal
  docker_flags+=("--tty")
fi

# copy environment variables except "$PATH" from host to the container
IFS=$'\n'
for line in $(env | grep -v '^PATH='); do
  docker_flags+=("-e" "$line")
done
unset IFS

docker run \
  "${docker_flags[@]}" \
  -w "$PWD" \
  -v "$PWD":"$PWD" \
  $image \
  "${@:2}"
