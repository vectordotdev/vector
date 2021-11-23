#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOAK_ROOT="${__dir}/.."

display_usage() {
    echo ""
    echo "Usage: run_experiment [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help: display this information"
    echo "  --soak: the experiment to run"
    echo "  --local-image: whether to use a local vector image or remote, local if true"
    echo "  --variant: the variation of test in play, either 'baseline' or 'comparison'"
    echo "  --tag: the tag this test covers"
    echo "  --capture-dir: the directory in which to write captures"
    echo "  --cpus: the total number of CPUs to dedicate to the soak minikube, default 7"
    echo "  --memory: the total amount of memory dedicate to the soak minikube, default 8g"
    echo "  --vector-cpus: the total number of CPUs to give to soaked vector"
    echo ""
}

while [[ $# -gt 0 ]]; do
  key="$1"

  case $key in
      --soak)
          SOAK_NAME=$2
          shift # past argument
          shift # past value
          ;;
      --variant)
          VARIANT=$2
          shift # past argument
          shift # past value
          ;;
      --image)
          IMAGE=$2
          shift # past argument
          shift # past value
          ;;
      --capture-dir)
          CAPTURE_DIR=$2
          shift # past argument
          shift # past value
          ;;
      --vector-cpus)
          VECTOR_CPUS=$2
          shift # past argument
          shift # past value
          ;;
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
          echo "unknown option: ${key}"
          display_usage
          exit 1
          ;;
  esac
done

WARMUP_GRACE=90
TOTAL_SAMPLES=120
SOAK_CAPTURE_DIR="${CAPTURE_DIR}/${SOAK_NAME}"

pushd "${__dir}"
./boot_minikube.sh --cpus "${SOAK_CPUS}" --memory "${SOAK_MEMORY}"
mkdir --parents "${SOAK_CAPTURE_DIR}"
minikube image load "${IMAGE}"
# Mount the capture directory. This is where the samples captured from inside
# the minikube will be placed on the host.
minikube mount "${SOAK_CAPTURE_DIR}:/captures" &
CAPTURE_MOUNT_PID=$!
popd

pushd "${SOAK_ROOT}/tests/${SOAK_NAME}"
mkdir --parents data
# Mount the data directory. This is where the data that supports the test are
# mounted into the minikube. The software running in the minikube will not write
# to this directory.
minikube mount "${SOAK_ROOT}/tests/${SOAK_NAME}/data:/data" &
DATA_MOUNT_PID=$!
popd

pushd "${SOAK_ROOT}/tests/${SOAK_NAME}/terraform"
terraform init
terraform apply -var "type=${VARIANT}" -var "vector_image=${IMAGE}" -var "vector_cpus=${VECTOR_CPUS}" -var "lading_image=ghcr.io/blt/lading:sha-0da91906d56acc899b829cea971d79f13e712e21" -auto-approve -compact-warnings -input=false -no-color
echo "[${VARIANT}] Captures will be recorded into ${SOAK_CAPTURE_DIR}"
echo "[${VARIANT}] Sleeping for ${WARMUP_GRACE} seconds to allow warm-up"
sleep "${WARMUP_GRACE}"
echo "[${VARIANT}] Recording captures to ${SOAK_CAPTURE_DIR}"
sleep "${TOTAL_SAMPLES}"
kill "${CAPTURE_MOUNT_PID}"
kill "${DATA_MOUNT_PID}"
popd

pushd "${__dir}"
./shutdown_minikube.sh
popd
