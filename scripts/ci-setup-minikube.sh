#!/bin/bash
set -euo pipefail

if [[ -z "${CI:-}" ]]; then
  echo "Aborted: this script is for use in CI, it may alter your system in an" \
    "unwanted way" >&2
  exit 1
fi

KUBERNETES_VERSION="${KUBERNETES_VERSION:?required}"
MINIKUBE_VERSION="${MINIKUBE_VERSION:?required}"
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:?required}"

set -x

curl -Lo kubectl \
  "https://storage.googleapis.com/kubernetes-release/release/${KUBERNETES_VERSION}/bin/linux/amd64/kubectl"
sudo install kubectl /usr/local/bin/ && rm kubectl

curl -Lo minikube \
  "https://storage.googleapis.com/minikube/releases/${MINIKUBE_VERSION}/minikube-linux-amd64"
sudo install minikube /usr/local/bin/ && rm minikube

minikube config set profile minikube
minikube config set vm-driver docker
minikube config set kubernetes-version "$KUBERNETES_VERSION"
minikube config set container-runtime "$CONTAINER_RUNTIME"

# Start minikube, try again once if fails and print logs if the second
# attempt fails too.
minikube start || { minikube delete && minikube start; } || minikube logs
kubectl cluster-info
