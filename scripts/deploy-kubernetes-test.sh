#!/usr/bin/env bash
set -euo pipefail

# deploy-kubernetes-test.sh
#
# SUMMARY
#
#   Deploys Vector into Kubernetes for testing purposes.
#   Uses the same installation method our users would use.
#
#   This script impements cli interface required by the kubernetes integration
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

# Allow overriding kubectl with somethingl like `minikube kubectl --`.
VECTOR_TEST_KUBECTL="${VECTOR_TEST_KUBECTL:-"kubectl"}"

# Allow optionally installing custom resource configs.
CUSTOM_RESOURCE_CONIFGS_FILE="${CUSTOM_RESOURCE_CONIFGS_FILE:-""}"


# TODO: replace with `helm template | kubectl apply -f -` when Helm Chart is
# available.

templated-config-global() {
  sed "s|^    namespace: vector|    namespace: $NAMESPACE|" < "distribution/kubernetes/vector-global.yaml" \
    | sed "s|^  name: vector|  name: $NAMESPACE|"
}

up() {
  # A Vector container image to use.
  CONTAINER_IMAGE="${CONTAINER_IMAGE:?"You must assing CONTAINER_IMAGE variable with the Vector container image name"}"

  templated-config-global | $VECTOR_TEST_KUBECTL create -f -

  $VECTOR_TEST_KUBECTL create namespace "$NAMESPACE"

  if [[ -n "$CUSTOM_RESOURCE_CONIFGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL create --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONIFGS_FILE"
  fi

  sed 's|image: timberio/vector:[^$]*$'"|image: $CONTAINER_IMAGE|" < "distribution/kubernetes/vector-namespaced.yaml" \
    | $VECTOR_TEST_KUBECTL create --namespace "$NAMESPACE" -f -
}

down() {
  $VECTOR_TEST_KUBECTL delete --namespace "$NAMESPACE" -f - < "distribution/kubernetes/vector-namespaced.yaml"

  if [[ -n "$CUSTOM_RESOURCE_CONIFGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL delete --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONIFGS_FILE"
  fi

  templated-config-global | $VECTOR_TEST_KUBECTL delete -f -

  $VECTOR_TEST_KUBECTL delete namespace "$NAMESPACE"
}

case "$COMMAND" in
  up|down)
    "$COMMAND" "$@"
    ;;
  *)
    echo "Invalid command: $COMMAND" >&2
    exit 1
esac
