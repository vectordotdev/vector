#!/usr/bin/env bash
set -euo pipefail

# test-e2e-kubernetes.sh
#
# SUMMARY
#
#   Run E2E tests for Kubernetes.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

random-string() {
  local CHARS="abcdefghijklmnopqrstuvwxyz0123456789"
  # shellcheck disable=SC2034
  for i in {1..8}; do
    echo -n "${CHARS:RANDOM%${#CHARS}:1}"
  done
  echo
}

# Detect if current kubectl context matches the one from `minikube`.
is_kubectl_context_minikube() {
  MINIKUBE_CLUSTER=$(minikube profile || true)
  [[ "$(kubectl config current-context || true)" == "${MINIKUBE_CLUSTER:-"minikube"}" ]]
}

# Detect if current kubectl context matches a `kind` cluster.
is_kubectl_context_kind() {
  for KIND_CLUSTER in $(kind get clusters || true); do
    if [[ "$(kubectl config current-context || true)" == "kind-${KIND_CLUSTER}" ]]; then
      return 0
    fi
  done
  return 1
}

# Whether to use `minikube cache` to pass image to the k8s cluster.
# After we build vector docker image, instead of pushing to the remote repo,
# we'll be using `minikube cache` to make image available to the cluster.
# This effectively eliminates the requirement to have a docker registry, but
# it requires that we run against minikube cluster.
is_minikube_cache_enabled() {
  local MODE="${USE_MINIKUBE_CACHE:-"auto"}"
  if [[ "$MODE" == "auto" ]]; then
    if is_kubectl_context_minikube; then
      echo "Note: detected minikube kubectl context, using minikube cache" >&2
      return 0
    else
      echo "Note: detected non-minikube kubectl context, docker repo is required" >&2
      return 1
    fi
  else
    [[ "$MODE" == "true" ]]
  fi
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
  # Hint #2: if using with minikube, set `USE_MINIKUBE_CACHE` to `true` and you
  # can omit the `CONTAINER_IMAGE_REPO`.
  #
  if is_minikube_cache_enabled; then
    # If `minikube cache` will be used, the push access to the docker repo
    # is not required, and we can provide a default value for the
    # `CONTAINER_IMAGE_REPO`.
    # CRIO prefixes the image name with `localhost/` when it's passed via
    # `minikube cache`, so, in our default value default, to work around that
    # issue, we use the repo name that already contains that prefix, such that
    # the effective image name on the minikube node matches the one expected in
    # tests.
    CONTAINER_IMAGE_REPO="${CONTAINER_IMAGE_REPO:-"localhost/vector-test"}"
  else
    # If not using `minikube cache`, it's mandatory to have a push access to the
    # repo, so we don't offer a default value and explicitly require the user to
    # specify a `CONTAINER_IMAGE_REPO`.
    CONTAINER_IMAGE_REPO="${CONTAINER_IMAGE_REPO:?"You have to specify CONTAINER_IMAGE_REPO to upload the test image to."}"
  fi

  # Assign a default test run ID if none is provided by the user.
  TEST_RUN_ID="${TEST_RUN_ID:-"$(date +%s)-$(random-string)"}"

  if [[ "${QUICK_BUILD:-"false"}" == "true" ]]; then
    # Prepare test image parameters.
    VERSION_TAG="test-$TEST_RUN_ID"

    # Prepare the container image for the deployment command and docker build.
    CONTAINER_IMAGE="$CONTAINER_IMAGE_REPO:$VERSION_TAG-debug"

    # Build docker image.
    docker build --build-arg RUST_VERSION="${RUST_VERSION}" --tag "$CONTAINER_IMAGE" -f tilt/Dockerfile .
  else
    # Package a .deb file to build a docker container, unless skipped.
    if [[ -z "${SKIP_PACKAGE_DEB:-}" ]]; then
      make package-deb-x86_64-unknown-linux-gnu
    fi

    # Prepare test image parameters.
    VERSION_TAG="test-$TEST_RUN_ID"
    BASE_TAG="debian"

    # Build docker image with Vector - the same way it's done for releases. Don't
    # do the push - we'll handle it later.
    REPO="$CONTAINER_IMAGE_REPO" \
      CHANNEL="test" \
      BASE="$BASE_TAG" \
      TAG="$VERSION_TAG" \
      PUSH="false" \
      scripts/build-docker.sh

    # Prepare the container image for the deployment command.
    CONTAINER_IMAGE="$CONTAINER_IMAGE_REPO:$VERSION_TAG-$BASE_TAG"
  fi
fi

if [[ -z "${SKIP_CONTAINER_IMAGE_PUBLISHING:-}" ]]; then
  # Make the container image accessible to the k8s cluster.
  if is_minikube_cache_enabled; then
    minikube image load "$CONTAINER_IMAGE"
    trap 'minikube image rm "$CONTAINER_IMAGE"' EXIT
  else
    docker push "$CONTAINER_IMAGE"
  fi
fi

# Export the container image to be accessible from the deployment command.
export CONTAINER_IMAGE

# Set the deployment commands for integration tests.
KUBE_TEST_DEPLOY_COMMAND="$(pwd)/scripts/deploy-chart-test.sh"
export KUBE_TEST_DEPLOY_COMMAND

# Prepare args.
CARGO_TEST_ARGS_CARGO=()
CARGO_TEST_ARGS_PASSED=()
if [[ -n "${SCOPE:-}" && "$SCOPE" != '""' ]]; then
  CARGO_TEST_ARGS_PASSED+=("$SCOPE")
fi
if [[ -n "${TEST:-}" && "$TEST" != '""' ]]; then
  CARGO_TEST_ARGS_CARGO+=(--test "$TEST")
else
  CARGO_TEST_ARGS_CARGO+=(--tests)
fi

# Run the tests.
cd lib/k8s-e2e-tests
cargo test \
  --no-fail-fast \
  --no-default-features \
  --features e2e-tests \
  "${CARGO_TEST_ARGS_CARGO[@]}" \
  -- \
  --nocapture \
  --test-threads 1 \
  "${CARGO_TEST_ARGS_PASSED[@]}"
