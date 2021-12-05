#!/bin/bash
set -euo pipefail

KUBERNETES_VERSION="v1.18.6"
HELM_VERSION="v3.2.4"

curl -Lo kubectl \
  "https://storage.googleapis.com/kubernetes-release/release/${KUBERNETES_VERSION}/bin/linux/amd64/kubectl"
sudo install kubectl /usr/local/bin/ && rm kubectl

curl -L "https://get.helm.sh/helm-${HELM_VERSION}-linux-amd64.tar.gz" \
  | tar -xzv --strip-components=1 --occurrence linux-amd64/helm
sudo install helm /usr/local/bin/ && rm helm

curl -L "https://github.com/instrumenta/kubeval/releases/latest/download/kubeval-linux-amd64.tar.gz" \
    | tar -xzv
sudo install kubeval /usr/local/bin/ && rm kubeval && rm README.md && rm LICENSE
