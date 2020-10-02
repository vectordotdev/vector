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
#   $ CONTAINER_IMAGE=timberio/vector:alpine-latest scripts/deploy-kubernetes-test.sh up vector-test-qwerty vector
#
#   Teardown:
#
#   $ scripts/deploy-kubernetes-test.sh down vector-test-qwerty vector
#

# Command to perform.
COMMAND="${1:?"Specify the command (up/down) as the first argument"}"

# A Kubernetes namespace to deploy to.
NAMESPACE="${2:?"Specify the namespace as the second argument"}"

if [[ "$COMMAND" == "up" ]]; then
  # The helm chart to deploy.
  HELM_CHART="${3:?"Specify the helm chart name as the third argument"}"
fi

# Allow overriding kubectl with something like `minikube kubectl --`.
VECTOR_TEST_KUBECTL="${VECTOR_TEST_KUBECTL:-"kubectl"}"

# Allow overriding helm with a custom command.
VECTOR_TEST_HELM="${VECTOR_TEST_HELM:-"helm"}"

# Allow optionally installing custom resource configs.
CUSTOM_RESOURCE_CONFIGS_FILE="${CUSTOM_RESOURCE_CONFIGS_FILE:-""}"

# Allow optionally passing custom Helm values.
CUSTOM_HELM_VALUES_FILE="${CUSTOM_HELM_VALUES_FILE:-""}"

split-container-image() {
  local INPUT="$1"
  CONTAINER_IMAGE_REPOSITORY="${INPUT%:*}"
  CONTAINER_IMAGE_TAG="${INPUT#*:}"
}

up() {
  # A Vector container image to use.
  CONTAINER_IMAGE="${CONTAINER_IMAGE:?"You must assign CONTAINER_IMAGE variable with the Vector container image name"}"

  $VECTOR_TEST_KUBECTL create namespace "$NAMESPACE"

  if [[ -n "$CUSTOM_RESOURCE_CONFIGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL create --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONFIGS_FILE"
  fi

  HELM_VALUES=()

  HELM_VALUES+=(
    # Set a reasonable log level to avoid issues with internal logs
    # overwriting console output.
    --set "env[0].name=LOG,env[0].value=info"
  )

  if [[ -n "$CUSTOM_HELM_VALUES_FILE" ]]; then
    HELM_VALUES+=(
      --values "$CUSTOM_HELM_VALUES_FILE"
    )
  fi

  split-container-image "$CONTAINER_IMAGE"
  HELM_VALUES+=(
    --set "image.repository=$CONTAINER_IMAGE_REPOSITORY"
    --set "image.tag=$CONTAINER_IMAGE_TAG"
  )

  set -x
  $VECTOR_TEST_HELM install \
    --atomic \
    --namespace "$NAMESPACE" \
    "${HELM_VALUES[@]}" \
    "vector" \
    "./distribution/helm/$HELM_CHART"
  { set +x; } &>/dev/null
}

down() {
  if [[ -n "$CUSTOM_RESOURCE_CONFIGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL delete --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONFIGS_FILE"
  fi

  $VECTOR_TEST_HELM delete --namespace "$NAMESPACE" "vector"

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
