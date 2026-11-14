#!/usr/bin/env bash
set -euo pipefail

# build-docker.sh
#
# SUMMARY
#
#   Builds the Vector docker images and optionally
#   pushes it to the Docker registry

set -x

vdev_cmd="${VDEV:-cargo vdev}"

CHANNEL="${CHANNEL:-"$($vdev_cmd release channel)"}"
VERSION="${VECTOR_VERSION:-"$($vdev_cmd version)"}"
DATE="${DATE:-"$(date -u +%Y-%m-%d)"}"
PLATFORM="${PLATFORM:-}"
PUSH="${PUSH:-"true"}"
# Load one platform into the local daemon for CI runtime tests without a push.
LOAD="${LOAD:-"false"}"
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
    elif [[ "$LOAD" == "true" ]]; then
      ARGS+=(--load)
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

  if [[ "$PUSH" == "false" && "$LOAD" == "true" ]]; then
    local SMOKE_CONFIG SMOKE_OUTPUT
    SMOKE_CONFIG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/tests/data/native-release-smoke.yaml"
    SMOKE_OUTPUT="$(mktemp)"
    local -a RUNTIME_ARGS=(--rm -i --volume "$SMOKE_CONFIG:/etc/vector/native-release-smoke.yaml:ro")
    if [[ -n "$BUILDABLE_PLATFORMS" ]]; then
      RUNTIME_ARGS+=(--platform="$BUILDABLE_PLATFORMS")
    fi
    printf 'native packaging works\n' | timeout 90 docker run "${RUNTIME_ARGS[@]}" \
      "${REPO_LIST[0]}:$VERSION-$BASE" --config /etc/vector/native-release-smoke.yaml > "$SMOKE_OUTPUT"
    grep -F '"message":"NATIVE PACKAGING WORKS"' "$SMOKE_OUTPUT"

    if [[ "$BUILDABLE_PLATFORMS" == "linux/arm/v6" ]]; then
      local QEMU_DIR VECTOR_BINARY
      QEMU_DIR="$(mktemp -d)"
      # Extract the emulator without installing it or registering binfmt handlers.
      # Only the completed runtime image runs under an emulated ARMv6 CPU.
      (
        cd "$QEMU_DIR"
        apt-get download qemu-user-static
        dpkg-deb --extract ./*.deb extracted
      )
      case "$BASE" in
        alpine) VECTOR_BINARY=/usr/local/bin/vector ;;
        debian) VECTOR_BINARY=/usr/bin/vector ;;
        *) echo "No ARMv6 runtime test for $BASE" >&2; return 1 ;;
      esac
      printf 'native packaging works\n' | timeout 90 docker run "${RUNTIME_ARGS[@]}" \
        --volume "$QEMU_DIR/extracted/usr/bin/qemu-arm-static:/ci-qemu-arm-static:ro" \
        --entrypoint /ci-qemu-arm-static "${REPO_LIST[0]}:$VERSION-$BASE" \
        -cpu arm1176 "$VECTOR_BINARY" --config /etc/vector/native-release-smoke.yaml > "$SMOKE_OUTPUT"
      grep -F '"message":"NATIVE PACKAGING WORKS"' "$SMOKE_OUTPUT"
      rm -rf "$QEMU_DIR"
    fi
    rm -f "$SMOKE_OUTPUT"
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
  # If BASE is specified, build only that base
  if [[ -n "${BASE:-}" ]]; then
    build "$BASE" "${TAG:-"test"}"
  else
    # If BASE is not specified, build all three bases
    build "alpine" "${TAG:-"test"}"
    build "distroless-libc" "${TAG:-"test"}"
    build "distroless-static" "${TAG:-"test"}"
  fi
fi
