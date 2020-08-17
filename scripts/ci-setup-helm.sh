#!/bin/bash
set -euo pipefail

if [[ -z "${CI:-}" ]]; then
  echo "Aborted: this script is for use in CI, it may alter your system in an" \
    "unwanted way" >&2
  exit 1
fi

set -x

KUBERNETES_VERSION="v1.18.6"
HELM_VERSION="v3.2.4"

curl -Lo kubectl \
  "https://storage.googleapis.com/kubernetes-release/release/${KUBERNETES_VERSION}/bin/linux/amd64/kubectl"
sudo install kubectl /usr/local/bin/ && rm kubectl

curl -L "https://get.helm.sh/helm-${HELM_VERSION}-linux-amd64.tar.gz" \
  | tar -xzv --strip-components=1 --occurrence linux-amd64/helm
sudo install helm /usr/local/bin/ && rm helm
