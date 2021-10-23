#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

display_usage() {
	echo -e "\nUsage: \$0 BASELINE_IMG COMPARISON_IMG\n"
}

if [  $# -le 0 ]
then
    display_usage
    exit 1
fi

BASELINE_IMG="${1:-}"
COMPARISON_IMG="${2:-}"

minikube stop || true
minikube delete || true
minikube start --cpus=7 --memory=8g

minikube image load "${BASELINE_IMG}"
minikube image load "${COMPARISON_IMG}"
