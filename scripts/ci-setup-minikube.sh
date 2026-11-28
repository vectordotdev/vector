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
GITHUB_TOKEN="${GITHUB_TOKEN:?required}"

# minikube checks unknown Kubernetes versions on GitHub without auth and can hit
# the rate limit. Check with auth here; `--force` below makes minikube ignore a
# failed lookup. Runs before `set -x` to keep the token out of the log.
curl -fsS --head -o /dev/null --retry 3 \
  -H "Authorization: Bearer ${GITHUB_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/kubernetes/kubernetes/releases/tags/${KUBERNETES_VERSION}" \
  || { echo "Kubernetes release ${KUBERNETES_VERSION} not found on GitHub" >&2; exit 1; }

set -x

curl -Lo kubectl \
  "https://dl.k8s.io/release/${KUBERNETES_VERSION}/bin/linux/amd64/kubectl"
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
minikube start --wait=all --force || { minikube delete && minikube start --wait=all --force; } || minikube logs
kubectl cluster-info
