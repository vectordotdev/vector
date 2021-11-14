#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

display_usage() {
    echo ""
    echo "Usage: boot_minikube [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --cpus: the total number of CPUs to dedicate to the soak minikube, default 7"
    echo "  --memory: the total amount of memory dedicate to the soak minikube, default 8g"
    echo ""
}

while [[ $# -gt 0 ]]; do
  key="$1"

  case $key in
      --cpus)
          SOAK_CPUS=$2
          shift # past argument
          shift # past value
          ;;
      --memory)
          SOAK_MEMORY=$2
          shift # past argument
          shift # past value
          ;;
      --help)
          display_usage
          exit 0
          ;;
      *)
          echo "unknown option"
          display_usage
          exit 1
          ;;
  esac
done

minikube stop || true
minikube delete || true
minikube start --cpus="${SOAK_CPUS}" --memory="${SOAK_MEMORY}" --driver=docker
