#!/usr/bin/env bash
set -euo pipefail

# build.sh
#
# SUMMARY
#
#   Used to build the variety of Docker images used to build, test, package,
#   and release Vector. This is primarily used in CI.

build-image() {
  local IMAGE_NAME="$1"
  shift

  local TAGS=("$@")

  FULL_TAGS=()
  for TAG in "${TAGS[@]}"; do
    FULL_TAGS+=("timberiodev/vector-$IMAGE_NAME:$TAG")
  done

  BUILD_ARGS_TAGS=()
  for FULL_TAG in "${FULL_TAGS[@]}"; do
    BUILD_ARGS_TAGS+=(-t "$FULL_TAG")
  done

  docker build \
    "${BUILD_ARGS_TAGS[@]}" \
    -f "scripts/ci-docker-images/$IMAGE_NAME/Dockerfile" \
    .

  for FULL_TAG in "${FULL_TAGS[@]}"; do
    docker push "$FULL_TAG"
  done

  if [[ "${LOW_DISK_SPACE:-"true"}" ]]; then
    docker image rm -f "${FULL_TAGS[@]}"
  fi
}

list-images() {
  find scripts/ci-docker-images -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort
}

ALL_IMAGES=()
while IFS='' read -r LINE; do ALL_IMAGES+=("$LINE"); done < <(list-images)

TAGS=(latest)
if [[ "${NIGHTLY_BUILD:-}" == "true" ]]; then
  DATE="$(date --iso)"
  SHA="$(git rev-parse --short HEAD)"
  TAGS+=("nightly" "nightly-$DATE" "nightly-$DATE-$SHA")
fi

for IMAGE in "${@:-"${ALL_IMAGES[@]}"}"; do
  build-image "$IMAGE" "${TAGS[@]}"
done
