#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

minikube stop || true
minikube delete || true
minikube start --cpus=7 --memory=8g --driver=docker
