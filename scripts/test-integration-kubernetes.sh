#!/usr/bin/env bash
set -euo pipefail

# test-integration-kubernetes.sh
#
# SUMMARY
#
#   Run integration tests for Kubernetes components only.

random-string() {
  local CHARS="abcdefghijklmnopqrstuvwxyz0123456789"
  # shellcheck disable=SC2034
  for i in {1..8}; do
    echo -n "${CHARS:RANDOM%${#CHARS}:1}"
  done
  echo
}

# Require a repo to put image at.
#
# Hint: you can use `scripts/start-docker-registry.sh`, but it requires
# manually preparing the environment to allow insecure registries, and it can
# also not work if you k8s cluster doesn't have network connectivity to the
# registry.
CONTAINER_IMAGE_REPO="${CONTAINER_IMAGE_REPO:?"You have to specify CONTAINER_IMAGE_REPO to upload the test image to."}"

# Assign a default test run ID if none is provided by the user.
TEST_RUN_ID="${TEST_RUN_ID:-"test-$(date +%s)-$(random-string)"}"

if [[ -z "${SKIP_PACKAGE_DEB:-}" ]]; then
  make package-deb-x86_64 USE_CONTAINER="${PACKAGE_DEB_USE_CONTAINER:-"docker"}"
fi

# Build docker image with Vector - the same way it's done for releses - and push
# it into the docker configured repo.
REPO="$CONTAINER_IMAGE_REPO" CHANNEL="test" TAG="$TEST_RUN_ID" PUSH=1 scripts/build-docker.sh

# Set the deployment command for integration tests.
export KUBE_TEST_DEPLOY_COMMAND="scripts/deploy-kubernetes-test.sh"

# Configure the deploy command to use our repo file.
export CONTAINER_IMAGE="$CONTAINER_IMAGE_REPO:$TEST_RUN_ID"

# TODO: enable kubernetes tests when they're implemented
exit 0 # disable the test and make them pass

# Run the tests.
cargo test --no-default-features --features kubernetes-integration-tests
