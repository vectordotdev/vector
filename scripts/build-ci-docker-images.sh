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
all_images=(
	builder-armv7-unknown-linux-gnueabihf
	builder-armv7-unknown-linux-musleabihf
	builder-x86_64-unknown-linux-gnu
	builder-x86_64-unknown-linux-musl
	checker
	packager-deb
	packager-rpm
	releaser
	verifier-amazonlinux-1
	verifier-amazonlinux-2
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

# The following images extend re-built cross base images. The end result
# is 2 new containers. See the README.md in the docker folder for more info.
# extend_cross_image "x86_64-unknown-linux-musl"
# extend_cross_image "x86_64-unknown-netbsd"