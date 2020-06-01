#!/usr/bin/env bash
set -euo pipefail

# test-e2e-kubernetes.sh
#
# SUMMARY
#
#   Run E2E tests for Kubernetes.

random-string() {
  local CHARS="abcdefghijklmnopqrstuvwxyz0123456789"
  # shellcheck disable=SC2034
  for i in {1..8}; do
    echo -n "${CHARS:RANDOM%${#CHARS}:1}"
  done
  echo
}

# Build a docker image if it wasn't provided.
if [[ -z "${CONTAINER_IMAGE:-}" ]]; then
  # Require a repo to put the container image at.
  #
  # Hint #1: you can use `scripts/start-docker-registry.sh`, but it requires
  # manually preparing the environment to allow insecure registries, and it can
  # also not work if you k8s cluster doesn't have network connectivity to the
  # registry.
  #
  # Hint #2: if using with minikube, set `USE_MINIKUBE_DOCKER` to `true` and use
  # any value for `CONTAINER_IMAGE_REPO` (for instance, `vector-test` will do).
  #
  CONTAINER_IMAGE_REPO="${CONTAINER_IMAGE_REPO:?"You have to specify CONTAINER_IMAGE_REPO to upload the test image to."}"

  # Assign a default test run ID if none is provided by the user.
  TEST_RUN_ID="${TEST_RUN_ID:-"$(date +%s)-$(random-string)"}"

  # Package a .deb file to build a docker container, unless skipped.
  if [[ -z "${SKIP_PACKAGE_DEB:-}" ]]; then
    make package-deb-x86_64 USE_CONTAINER="${PACKAGE_DEB_USE_CONTAINER:-"docker"}"
  fi

  # Prepare test image parameters.
  VERSION_TAG="test-$TEST_RUN_ID"
  BASE_TAG="debian"

  # Build docker image with Vector - the same way it's done for releses. Don't
  # do the push - we'll handle it later.
  REPO="$CONTAINER_IMAGE_REPO" \
    CHANNEL="test" \
    BASE="$BASE_TAG" \
    TAG="$VERSION_TAG" \
    PUSH="" \
    scripts/build-docker.sh

  # Prepare the container image for the deployment command.
  CONTAINER_IMAGE="$CONTAINER_IMAGE_REPO:$VERSION_TAG-$BASE_TAG"
fi

if [[ -z "${SKIP_CONTAINER_IMAGE_PUBLISHING:-}" ]]; then
  # Whether to use minikube docker.
  # After we build vector docker image, instead of pushing to the remote repo,
  # we'll be exporting it to a file after (from the "host" docker engine), and
  # then importing that file into the minikube in-cluster docker engine, that
  # nodes have access to.
  # This effectively eliminates the requirement to have a docker registry, but
  # it requires that we run against minikube cluster.
  USE_MINIKUBE_DOCKER="${USE_MINIKUBE_DOCKER:-"false"}"

  # Make the container image accessible to the k8s cluster.
  if [[ "$USE_MINIKUBE_DOCKER" == "true" ]]; then
    scripts/copy-docker-image-to-minikube.sh "$CONTAINER_IMAGE"
  else
    docker push "$CONTAINER_IMAGE"
  fi
fi

# Export the container image to be accessible from the deployment command.
export CONTAINER_IMAGE

# Set the deployment command for integration tests.
export KUBE_TEST_DEPLOY_COMMAND="scripts/deploy-kubernetes-test.sh"

# Run the tests.
cargo test --test kubernetes-e2e --no-default-features --features kubernetes-e2e-tests -- --nocapture
