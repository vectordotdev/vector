#!/usr/bin/env bash
set -euo pipefail

# copy-docker-image-to-minikube.sh
#
# SUMMARY
#
#   Copies a list of images from the host docker engine to the minikube docker
#   engine via save/load commands.
#
#   Requires minikube and docker to be available.
#
# USAGE
#
#   copy-docker-image-to-minikube.sh timberio/vector:latest

# Image to copy.
IMAGES=("${@:?"Specify the images to copy in the arguments list"}")

# Prepare temp dir to store the images archive.
TD="$(mktemp -d)"
IMAGES_ARCHIVE="$TD/images.tar.gz"

# Save images.
docker save "${IMAGES[@]}" | gzip >"$IMAGES_ARCHIVE"

# Start a subshell to preserve the env state.
(
  # Switch to minikube docker.
  eval "$(minikube --shell bash docker-env)"

  # Load images.
  docker load -i "$IMAGES_ARCHIVE"
)

# Clear temp dir.
rm -rf "$TD"
