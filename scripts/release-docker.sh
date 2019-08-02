#!/usr/bin/env bash

# release-docker.sh
#
# SUMMARY
#
#   Builds and pushes Vector docker images

set -eu

echo "Releasing timberio/vector* Docker images"
docker build -t timberio/vector-alpine:$VERSION distribution/docker/alpine
docker build -t timberio/vector-debian:$VERSION distribution/docker/debian
docker build -t timberio/vector-debian-slim:$VERSION distribution/docker/debian-slim
docker build -t timberio/vector-alpine:latest distribution/docker/alpine
docker build -t timberio/vector-debian:latest distribution/docker/debian
docker build -t timberio/vector-debian-slim:latest distribution/docker/debian-slim

docker login -u "$DOCKER_USERNAME" -p "$DOCKER_PASSWORD"
docker push timberio/vector-alpine:$VERSION
docker push timberio/vector-debian:$VERSION
docker push timberio/vector-debian-slim:$VERSION
docker push timberio/vector-alpine:latest
docker push timberio/vector-debian:latest
docker push timberio/vector-debian-slim:latest