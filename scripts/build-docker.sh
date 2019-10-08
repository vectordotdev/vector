#!/usr/bin/env bash

# build-docker.sh
#
# SUMMARY
#
#   Builds the Vector docker images

set -eu

# saner programming env: these switches turn some bugs into errors
set -o errexit -o pipefail -o noclobber -o nounset

CHANNEL=$(scripts/util/release-channel.sh)

#
# Functions
#

verify() {
  container_id=$(docker run -d $1)
  sleep 2
  state=$(docker inspect $container_id -f {{.State.Running}})

  if [[ "$state" != "true" ]]; then
    echo "Docker container failed to start"
    exit 1
  fi

  docker stop $container_id
}

#
# Prepare
#

cp vector-x86_64-unknown-linux-musl.tar.gz distribution/docker/alpine
cp vector-amd64.deb distribution/docker/debian

#
# Build
#

echo "Building timberio/vector:* Docker images"

if [[ "$CHANNEL" == "latest" ]]; then
  docker build --build-arg version=$VERSION --tag timberio/vector:$VERSION-alpine distribution/docker/alpine
  docker build --build-arg version=$VERSION --tag timberio/vector:latest-alpine distribution/docker/alpine
  docker build --build-arg version=$VERSION --tag timberio/vector:$VERSION-debian distribution/docker/debian
  docker build --build-arg version=$VERSION --tag timberio/vector:latest-debian distribution/docker/debian
elif [[ "$CHANNEL" == "nightly" ]]; then
  docker build --build-arg version=$VERSION --tag timberio/vector:nightly-alpine distribution/docker/alpine
  docker build --build-arg version=$VERSION --tag timberio/vector:nightly-debian distribution/docker/debian
fi

#
# Verify
#

if [[ "$CHANNEL" == "latest" ]]; then
  verify timberio/vector:$VERSION-alpine
  verify timberio/vector:latest-alpine
  verify timberio/vector:$VERSION-debian
  verify timberio/vector:latest-debian
elif [[ "$CHANNEL" == "nightly" ]]; then
  verify timberio/vector:nightly-alpine
  verify timberio/vector:nightly-debian
fi