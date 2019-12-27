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
  version_exact=$VERSION
  version_minor_x=$(echo $VERSION | sed 's/\.[0-9]*$/.X/g')
  version_major_x=$(echo $VERSION | sed 's/\.[0-9]*\.[0-9]*$/.X/g')

  for i in $version_exact $version_minor_x $version_major_x latest; do
    build alpine $i
    build debian $i
  done
elif [[ "$CHANNEL" == "nightly" ]]; then
  for i in nightly-$DATE nightly; do
    build alpine $i
    build debian $i
  done
fi
