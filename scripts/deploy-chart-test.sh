#!/usr/bin/env bash
set -euo pipefail

# deploy-public-chart-test.sh
#
# SUMMARY
#
#   Deploys a public chart into Kubernetes for testing purposes.
#   Uses the same installation method our users would use.
#
#   This script implements cli interface required by the kubernetes E2E
#   tests.
#
# USAGE
#
#   Deploy:
#
#   $ CHART_REPO=https://helm.testmaterial.tld scripts/deploy-public-chart-test.sh up test-namespace-qwerty chart release
#
#   Teardown:
#
#   $ scripts/deploy-public-chart-test.sh down test-namespace-qwerty chart
#

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Command to perform.
COMMAND="${1:?"Specify the command (up/down) as the first argument"}"

# A Kubernetes namespace to deploy to.
NAMESPACE="${2:?"Specify the namespace as the second argument"}"

# The helm chart to manage
HELM_CHART="${3:?"Specify the helm chart name as the third argument"}"

# Release name for chart install
RELEASE_NAME="${4:?"Specify the release name as the fourth argument"}"

# Allow overriding kubectl with something like `minikube kubectl --`.
VECTOR_TEST_KUBECTL="${VECTOR_TEST_KUBECTL:-"kubectl"}"

# Allow overriding helm with a custom command.
VECTOR_TEST_HELM="${VECTOR_TEST_HELM:-"helm"}"

# Allow optionally installing custom resource configs.
CUSTOM_RESOURCE_CONFIGS_FILE="${CUSTOM_RESOURCE_CONFIGS_FILE:-""}"

# Allow optionally passing custom Helm values.
CUSTOM_HELM_VALUES_FILES="${CUSTOM_HELM_VALUES_FILES:-""}"

# Allow overriding the local repo name, useful to use multiple external repo
CUSTOM_HELM_REPO_LOCAL_NAME="${CUSTOM_HELM_REPO_LOCAL_NAME:-"local_repo"}"

split-container-image() {
  local INPUT="$1"
  CONTAINER_IMAGE_REPOSITORY="${INPUT%:*}"
  CONTAINER_IMAGE_TAG="${INPUT##*:}"
}

up() {
  # A Vector container image to use.
  CONTAINER_IMAGE="${CONTAINER_IMAGE:?"You must assign CONTAINER_IMAGE variable with the Vector container image name"}"

  $VECTOR_TEST_HELM repo add "$CUSTOM_HELM_REPO_LOCAL_NAME" "$CHART_REPO" --force-update || true
  $VECTOR_TEST_HELM repo update

  $VECTOR_TEST_KUBECTL create namespace "$NAMESPACE" --dry-run -o yaml | $VECTOR_TEST_KUBECTL apply -f -

  if [[ -n "$CUSTOM_RESOURCE_CONFIGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL create --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONFIGS_FILE"
  fi


  HELM_VALUES=()

  for file in $CUSTOM_HELM_VALUES_FILES ; do
    HELM_VALUES+=(
      --values "$file"
    )
  done

  # Set a reasonable log level to avoid issues with internal logs
  # overwriting console output.
  split-container-image "$CONTAINER_IMAGE"
  HELM_VALUES+=(
    --set "env[0].name=VECTOR_LOG"
    --set "env[0].value=info"
    --set "image.repository=$CONTAINER_IMAGE_REPOSITORY"
    --set "image.tag=$CONTAINER_IMAGE_TAG"
  )

  set -x
  $VECTOR_TEST_HELM install \
    --atomic \
    --namespace "$NAMESPACE" \
    "${HELM_VALUES[@]}" \
    "$RELEASE_NAME" \
    "$CUSTOM_HELM_REPO_LOCAL_NAME/$HELM_CHART"
  { set +x; } &>/dev/null
}

down() {
  if [[ -n "$CUSTOM_RESOURCE_CONFIGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL delete --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONFIGS_FILE"
  fi

  if $VECTOR_TEST_HELM status --namespace "$NAMESPACE" "$HELM_CHART" &>/dev/null; then
    $VECTOR_TEST_HELM delete --namespace "$NAMESPACE" "$HELM_CHART"
  fi

  if $VECTOR_TEST_KUBECTL get namespace "$NAMESPACE" &>/dev/null; then
    $VECTOR_TEST_KUBECTL delete namespace "$NAMESPACE"
  fi
}

case "$COMMAND" in
  up|down)
    "$COMMAND" "$@"
    ;;
  *)
    echo "Invalid command: $COMMAND" >&2
    exit 1
esac
