#!/usr/bin/env bash

# build-docker.sh
#
# SUMMARY
#
#   Builds the Vector docker images

set -eux

CHANNEL=$(scripts/util/release-channel.sh)
VERSION=$(scripts/version.sh)
DATE=$(date -u +%Y-%m-%d)

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
    -f distribution/docker/$base/Dockerfile
}

verify() {
  tag=$1
  container_id=$(docker run -d $tag)
  sleep 2
  state=$(docker inspect $container_id -f {{.State.Running}})

  if [[ "$state" != "true" ]]; then
    echo "Docker container $tag failed to start"
    exit 1
  fi

  docker stop $container_id

  echo "Docker container $tag started successfully"
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

#
# Verify
#

# Images built using `buildx` are invisible to `docker run`

# if [[ "$CHANNEL" == "latest" ]]; then
#   verify timberio/vector:$VERSION-alpine
#   verify timberio/vector:latest-alpine
#   verify timberio/vector:$VERSION-debian
#   verify timberio/vector:latest-debian
# elif [[ "$CHANNEL" == "nightly" ]]; then
#   verify timberio/vector:nightly-alpine
#   verify timberio/vector:nightly-$DATE-alpine
#   verify timberio/vector:nightly-debian
#   verify timberio/vector:nightly-$DATE-debian
# fi