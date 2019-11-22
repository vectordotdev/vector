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

docker_flags="--privileged --interactive"
if [ -t 0 ]; then # the script's input is connected to a terminal
  docker_flags="$docker_flags --tty"
fi
docker run \
  $docker_flags \
  $(env | xargs -n1 printf '-e "%s"\n') \
  -w "$PWD" \
  -v "$PWD":"$PWD" \
  $image \
  "${@:2}"