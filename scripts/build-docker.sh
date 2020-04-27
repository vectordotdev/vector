#!/usr/bin/env bash
set -euo pipefail

# build-docker.sh
#
# SUMMARY
#
#   Builds the Vector docker images and optionally
#   pushes it to the Docker registry

set -x

CHANNEL="${CHANNEL:-"$(scripts/util/release-channel.sh)"}"
VERSION="${VERSION:-"$(scripts/version.sh)"}"
DATE="${DATE:-"$(date -u +%Y-%m-%d)"}"
PUSH="${PUSH:-}"
PLATFORM="${PLATFORM:-}"
REPO="${REPO:-"timberio/vector"}"

#
# Functions
#

build() {
  local BASE="$1"
  local VERSION="$2"

  local TAG="$REPO:$VERSION-$BASE"
  local DOCKERFILE="distribution/docker/$BASE/Dockerfile"

  if [ -n "$PLATFORM" ]; then
    export DOCKER_CLI_EXPERIMENTAL=enabled
    docker run --rm --privileged docker/binfmt:66f9012c56a8316f9244ffd7622d7c21c1f6f28d
    docker buildx rm vector-builder || true
    docker buildx create --use --name vector-builder
    docker buildx install

    docker buildx build \
      --platform="$PLATFORM" \
      --tag "$TAG" \
      target/artifacts \
      -f "$DOCKERFILE" ${PUSH:+--push}
  else
    docker build \
      --tag "$TAG" \
      target/artifacts \
      -f "$DOCKERFILE"

    if [ -n "$PUSH" ]; then
      docker push "$TAG"
    fi
  fi
}

#
# Build
#

echo "Building $REPO:* Docker images"

if [[ "$CHANNEL" == "latest" ]]; then
  VERSION_EXACT="$VERSION"
  # shellcheck disable=SC2001
  VERSION_MINOR_X=$(echo "$VERSION" | sed 's/\.[0-9]*$/.X/g')
  # shellcheck disable=SC2001
  VERSION_MAJOR_X=$(echo "$VERSION" | sed 's/\.[0-9]*\.[0-9]*$/.X/g')

  for VERSION_TAG in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X" latest; do
    build alpine "$VERSION_TAG"
    build debian "$VERSION_TAG"
  done
elif [[ "$CHANNEL" == "nightly" ]]; then
  for VERSION_TAG in "nightly-$DATE" nightly; do
    build alpine "$VERSION_TAG"
    build debian "$VERSION_TAG"
  done
elif [[ "$CHANNEL" == "test" ]]; then
  build "${BASE:-"alpine"}" "${TAG:-"test"}"
fi
