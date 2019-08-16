#!/usr/bin/env bash

# release-docker.sh
#
# SUMMARY
#
#   Builds and pushes Vector docker images

set -eu

CHANNEL=$(scripts/util/release-channel.sh)

#
# Build
#

echo "Building timberio/vector:* Docker images"

docker build -t timberio/vector:$VERSION-alpine distribution/docker/alpine
docker build -t timberio/vector:$VERSION-debian distribution/docker/debian
docker build -t timberio/vector:$VERSION-debian-slim distribution/docker/debian-slim

if [[ "$CHANNEL" == "latest" ]]; then
  docker build -t timberio/vector:latest-alpine distribution/docker/alpine
  docker build -t timberio/vector:latest-debian distribution/docker/debian
  docker build -t timberio/vector:latest-debian-slim distribution/docker/debian-slim
elif [[ "$CHANNEL" == "nightly" ]]; then
  docker build -t timberio/vector:nightly-alpine distribution/docker/alpine
  docker build -t timberio/vector:nightly-debian distribution/docker/debian
  docker build -t timberio/vector:nightly-debian-slim distribution/docker/debian-slim
fi

#
# Pushing
#

echo "Pushing timberio/vector Docker images"
docker login -u "$DOCKER_USERNAME" -p "$DOCKER_PASSWORD"
docker push timberio/vector:$VERSION-alpine
docker push timberio/vector:$VERSION-debian
docker push timberio/vector:$VERSION-debian-slim

if [[ "$CHANNEL" == "latest" ]]; then
  docker push timberio/vector:latest-alpine
  docker push timberio/vector:latest-debian
  docker push timberio/vector:latest-debian-slim
elif [[ "$CHANNEL" == "nightly" ]]; then
  docker push timberio/vector:nightly-alpine
  docker push timberio/vector:nightly-debian
  docker push timberio/vector:nightly-debian-slim
fi