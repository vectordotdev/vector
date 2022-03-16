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
minikube start \
  --feature-gates="CPUManager=true" `# enable CPU management flags` \
  --extra-config="kubelet.cpu-manager-policy=static" `# use static policy to let "Guaranteed" pods (those with limits=requests) and integer CPU requests have CPU affinity` \
  --extra-config "kubelet.kube-reserved=cpu=1" `# reserve 1 CPU for kube` \
  --cpus="${SOAK_CPUS}" `# ensure this is higher than the number of CPUs requested by all soak pods or you will see throttling by CFS via the docker driver` \
  --memory="${SOAK_MEMORY}" `# ensure this is higher than the amount of memory requested by all soak pods or you will see OOMs` \
  --driver=docker
