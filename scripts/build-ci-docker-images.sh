#!/usr/bin/env bash
set -euo pipefail

# build.sh
#
# SUMMARY
#
#   Used to build the variety of Docker images used to build, test, package,
#   and release Vector. This is primarily used in CI.

# Builds a generic Docker image with a `vector-` prefix. The name
# maps to the contained folder.
build_image() {
  local TAG="$1"

  docker build \
    -t "timberiodev/vector-$TAG:latest" \
    -f "scripts/ci-docker-images/$TAG/Dockerfile" \
    .

  docker push "timberiodev/vector-$TAG:latest"
}

# The following images are basic Docker images that do not extend a
# cross base image.
ALL_IMAGES=(
  build-aarch64-unknown-linux-musl
  builder-x86_64-unknown-linux-gnu
  builder-x86_64-unknown-linux-musl
  checker
  packager-rpm
  releaser
  verifier-amazonlinux-1
  verifier-amazonlinux-2
  verifier-centos-7
  verifier-deb-8
  verifier-deb-9
  verifier-deb-10
  verifier-ubuntu-16-04
  verifier-ubuntu-18-04
  verifier-ubuntu-19-04
)

for IMAGE in "${@:-"${ALL_IMAGES[@]}"}"; do
  build_image "$IMAGE"
done
