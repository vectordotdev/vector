#!/usr/bin/env bash

# release-docker.sh
#
# SUMMARY
#
#   Builds and pushes Vector docker images

set -eu

echo "Releasing timberio/vector Docker images"
docker build -t timberio/vector:$VERSION-alpine distribution/docker/alpine
docker build -t timberio/vector:$VERSION-debian distribution/docker/debian
docker build -t timberio/vector:$VERSION-debian-slim distribution/docker/debian-slim
docker build -t timberio/vector:latest-alpine distribution/docker/alpine
docker build -t timberio/vector:latest-debian distribution/docker/debian
docker build -t timberio/vector:latest-debian-slim distribution/docker/debian-slim

docker login -u "$DOCKER_USERNAME" -p "$DOCKER_PASSWORD"
docker push timberio/vector:$VERSION-alpine
docker push timberio/vector:$VERSION-debian
docker push timberio/vector:$VERSION-debian-slim
docker push timberio/vector:latest-alpine
docker push timberio/vector:latest-debian
docker push timberio/vector:latest-debian-slim