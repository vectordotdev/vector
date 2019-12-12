#!/usr/bin/env bash

# build-docker.sh
#
# SUMMARY
#
#   Builds the Vector docker images and optionally
#   pushes it to the Docker registry

set -eux

CHANNEL=$(scripts/util/release-channel.sh)
VERSION=$(scripts/version.sh)
DATE=$(date -u +%Y-%m-%d)
PUSH=${PUSH:-}
PLATFORM=${PLATFORM:-linux/amd64,linux/arm64,linux/arm}

#
# Functions
#

build() {
  base=$1
  version=$2

  docker buildx build \
    --platform="$PLATFORM" \
    --tag timberio/vector:$version-$base \
    target/artifacts \
    -f distribution/docker/$base/Dockerfile ${PUSH:+--push}
}

#
# Build
#

echo "Building timberio/vector:* Docker images"

export DOCKER_CLI_EXPERIMENTAL=enabled
docker run --rm --privileged docker/binfmt:66f9012c56a8316f9244ffd7622d7c21c1f6f28d
docker buildx rm vector-builder || true
docker buildx create --use --name vector-builder
docker buildx install

if [[ "$CHANNEL" == "latest" ]]; then
  build alpine latest
  build alpine $VERSION
  build debian latest
  build debian $VERSION
elif [[ "$CHANNEL" == "nightly" ]]; then
  build alpine nightly
  build alpine nightly-$DATE
  build debian nightly
  build debian nightly-$DATE
fi