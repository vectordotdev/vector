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
    scripts/ci-docker-images

  docker push timberiodev/vector-$tag:latest
}

# This function:
#
# 1. Re-builds a fresh cross base image, tagged with our own name.
#    Ex: `timberiodev/vector-builder-base-x86_64-apple-darwin`
# 2. Builds our own target image that extends the new above image.
#    Ex: `timberiodev/vector-builder-x86_64-apple-darwin`
#
# See the README.md in the docker folder for more info.
function extend_cross_base_image() {
  local target=$1

  docker build \
    -t timberiodev/vector-builder-base-$target:latest \
    -f $target/Dockerfile \
    github.com/rust-embedded/cross#:docker

  docker push timberiodev/vector-builder-base-$target:latest
  build_image "builder-$target"
}

# The following images are basic Docker images that do not extend a
# cross base image.
build_image "builder-armv7-unknown-linux-gnueabihf"
build_image "builder-armv7-unknown-linux-musleabihf"
build_image "builder-x86_64-unknown-linux-gnu"
#build_image "builder-x86_64-unknown-linux-musl"
build_image "checker"
build_image "packager-deb"
build_image "packager-rpm"
build_image "releaser"
build_image "verifier-amazonlinux-1"
build_image "verifier-amazonlinux-2"
build_image "verifier-deb-8"
build_image "verifier-deb-9"
build_image "verifier-deb-10"
build_image "verifier-ubuntu-16-04"
build_image "verifier-ubuntu-18-04"
build_image "verifier-ubuntu-19-04"

# The following images extend re-built cross base images. The end result
# is 2 new containers. See the README.md in the docker folder for more info.
# extend_cross_image "x86_64-unknown-linux-musl"
# extend_cross_image "x86_64-unknown-netbsd"