#!/usr/bin/env bash

# build.sh
#
# SUMMARY
#
#   Used to build the variety of Docker images used to build, test, package,
#   and release Vector. This is primarily used in CI.

set -eou pipefail

# Builds a generic Docker image with a `vector-` prefix. The name
# maps to the contained folder.
function build_image() {
  local tag=$1

  docker build \
    -t timberiodev/vector-$tag:latest \
    -f scripts/ci-docker-images/$tag/Dockerfile \
    .

  docker push timberiodev/vector-$tag:latest
}

# The following images are basic Docker images that do not extend a
# cross base image.
all_images=(
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
for image in ${*:-${all_images[*]}}
do
  build_image $image
done
