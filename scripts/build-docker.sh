#!/usr/bin/env bash
set -euo pipefail

# build-docker.sh
#
# SUMMARY
#
#   Builds the Vector docker images and optionally
#   pushes it to the Docker registry

set -x

CHANNEL="${CHANNEL:-"$(cargo vdev release channel)"}"
VERSION="${VECTOR_VERSION:-"$(cargo vdev version)"}"
DATE="${DATE:-"$(date -u +%Y-%m-%d)"}"
PLATFORM="${PLATFORM:-}"
PUSH="${PUSH:-"true"}"
REPOS="${REPOS:-"timberio/vector"}"
IFS=, read -ra REPO_LIST <<< "$REPOS"

IFS=, read -ra REQUESTED_PLATFORMS <<< "$PLATFORM"
declare -A SUPPORTED_PLATFORMS=(
  [debian]="linux/amd64,linux/arm/v6,linux/arm/v7,linux/arm64/v8"
  [alpine]="linux/amd64,linux/arm/v6,linux/arm/v7,linux/arm64/v8"
  [distroless-static]="linux/amd64,linux/arm/v7,linux/arm64/v8"
  [distroless-libc]="linux/amd64,linux/arm/v7,linux/arm64/v8"
)

#
# Functions
#

evaluate_supported_platforms_for_base() {
  local BASE="$1"
  IFS=, read -ra SUPPORTED_PLATFORMS_FOR_BASE <<< "${SUPPORTED_PLATFORMS["$BASE"]}"

  local BUILDABLE_PLATFORMS=""
  for platform in "${REQUESTED_PLATFORMS[@]}"
  do
    if [[ ${SUPPORTED_PLATFORMS_FOR_BASE[*]} =~ $platform ]]
    then
      BUILDABLE_PLATFORMS+="$platform,"
    else
      >&2 echo "WARN: skipping $platform for $BASE, no base image for platform"
    fi
  done

  echo "${BUILDABLE_PLATFORMS%?}"
}

build() {
  local BASE="$1"
  local VERSION="$2"
  local DOCKERFILE="distribution/docker/$BASE/Dockerfile"
  local BUILDABLE_PLATFORMS=""
  if [ -n "$PLATFORM" ]; then
    BUILDABLE_PLATFORMS=$(evaluate_supported_platforms_for_base "$BASE")
  fi

  # Collect all tags
  TAGS=()
  for REPO in "${REPO_LIST[@]}"; do
    TAGS+=(--tag "$REPO:$VERSION-$BASE")
  done

  # Build once with all tags
  if [ -n "$PLATFORM" ]; then
    ARGS=()
    if [[ "$PUSH" == "true" ]]; then
      ARGS+=(--push)
    fi

    docker buildx build \
      --platform="$BUILDABLE_PLATFORMS" \
      "${TAGS[@]}" \
      target/artifacts \
      -f "$DOCKERFILE" \
      "${ARGS[@]}"
  else
    docker build \
      "${TAGS[@]}" \
      target/artifacts \
      -f "$DOCKERFILE"

    if [[ "$PUSH" == "true" ]]; then
      for TAG in "${TAGS[@]}"; do
        docker push "$TAG"
      done
    fi
  fi
}

#
# Build
#

echo "Building Docker images for $REPOS"

if [[ "$CHANNEL" == "release" ]]; then
  VERSION_EXACT="$VERSION"
  # shellcheck disable=SC2001
  VERSION_MINOR_X=$(echo "$VERSION" | sed 's/\.[0-9]*$/.X/g')
  # shellcheck disable=SC2001
  VERSION_MAJOR_X=$(echo "$VERSION" | sed 's/\.[0-9]*\.[0-9]*$/.X/g')

  for VERSION_TAG in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X" latest; do
    build alpine "$VERSION_TAG"
    build debian "$VERSION_TAG"
    build distroless-static "$VERSION_TAG"
    build distroless-libc "$VERSION_TAG"
  done
elif [[ "$CHANNEL" == "nightly" ]]; then
  for VERSION_TAG in "nightly-$DATE" nightly; do
    build alpine "$VERSION_TAG"
    build debian "$VERSION_TAG"
    build distroless-static "$VERSION_TAG"
    build distroless-libc "$VERSION_TAG"
  done
elif [[ "$CHANNEL" == "custom" ]]; then
  build alpine "$VERSION"
  build debian "$VERSION"
  build distroless-static "$VERSION"
  build distroless-libc "$VERSION"
elif [[ "$CHANNEL" == "test" ]]; then
  build "${BASE:-"alpine"}" "${TAG:-"test"}"
  build "${BASE:-"distroless-libc"}" "${TAG:-"test"}"
  build "${BASE:-"distroless-static"}" "${TAG:-"test"}"
fi
