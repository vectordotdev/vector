#!/usr/bin/env bash
set -euo pipefail

# deploy-kubernetes-test.sh
#
# SUMMARY
#
#   Deploys Vector into Kubernetes for testing purposes.
#   Uses the same installation method our users would use.
#
#   This script implements cli interface required by the kubernetes E2E
#   tests.
#
# USAGE
#
#   Deploy:
#
#   $ CONTAINER_IMAGE=timberio/vector:alpine-latest scripts/deploy-kubernetes-test.sh up vector-test-qwerty
#
#   Teardown:
#
#   $ scripts/deploy-kubernetes-test.sh down vector-test-qwerty
#

# Command to perform.
COMMAND="${1:?"Specify the command (up/down) as the first argument"}"

# A Kubernetes namespace to deploy to.
NAMESPACE="${2:?"Specify the namespace as the second argument"}"

# Allow overriding kubectl with something like `minikube kubectl --`.
VECTOR_TEST_KUBECTL="${VECTOR_TEST_KUBECTL:-"kubectl"}"

# Allow optionally installing custom resource configs.
CUSTOM_RESOURCE_CONFIGS_FILE="${CUSTOM_RESOURCE_CONFIGS_FILE:-""}"


# TODO: replace with `helm template | kubectl apply -f -` when Helm Chart is
# available.

templated-config() {
  cat < "distribution/kubernetes/vector.yaml" \
    | sed "s|^    namespace: vector|    namespace: $NAMESPACE|"
}

up() {
  # A Vector container image to use.
  CONTAINER_IMAGE="${CONTAINER_IMAGE:?"You must assign CONTAINER_IMAGE variable with the Vector container image name"}"

  $VECTOR_TEST_KUBECTL create namespace "$NAMESPACE"

  if [[ -n "$CUSTOM_RESOURCE_CONFIGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL create --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONFIGS_FILE"
  fi

  templated-config \
    | sed -E 's|image: "?timberio/vector:[^$]*$'"|image: $CONTAINER_IMAGE|" \
    | $VECTOR_TEST_KUBECTL create --namespace "$NAMESPACE" -f -
}

down() {
  if [[ -n "$CUSTOM_RESOURCE_CONFIGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL delete --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONFIGS_FILE"
  fi

  templated-config | $VECTOR_TEST_KUBECTL delete --namespace "$NAMESPACE" -f -

  $VECTOR_TEST_KUBECTL delete namespace "$NAMESPACE"
}

case "$COMMAND" in
  up|down|templated-config)
    "$COMMAND" "$@"
    ;;
  *)
    echo "Invalid command: $COMMAND" >&2
    exit 1
esac
