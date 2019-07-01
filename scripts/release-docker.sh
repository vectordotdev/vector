#!/usr/bin/env bash

# release-docker.sh
#
# SUMMARY
#
#   Builds and pushes Vector docker images

set -eu

echo "Releasing timberio/vector* Docker images"
docker build -t timberio/vector:$VERSION distribution/docker
docker build -t timberio/vector-slim:$VERSION distribution/docker/slim
docker build -t timberio/vector:latest distribution/docker
docker build -t timberio/vector-slim:latest distribution/docker/slim

docker login -u "$DOCKER_USERNAME" -p "$DOCKER_PASSWORD"
docker push timberio/vector:$VERSION
docker push timberio/vector-slim:$VERSION
docker push timberio/vector:latest
docker push timberio/vector-slim:latest