#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOAK_ROOT="${__dir}/.."

display_usage() {
    echo ""
    echo "Usage: soak_one [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help: display this information"
    echo "  --soak: the experiment to run"
    echo "  --build-image: build the soak image if needed, default true"
    echo "  --variant: the variation of test in play, either 'baseline' or 'comparison'"
    echo "  --tag: the tag this test covers"
    echo "  --capture-dir: the directory in which to write captures"
    echo "  --cpus: the total number of CPUs to dedicate to the soak minikube, default 7"
    echo "  --memory: the total amount of memory dedicate to the soak minikube, default 8g"
    echo "  --vector-cpus: the total number of CPUs to give to soaked vector"
    echo "  --warmup-seconds: the total number seconds to pause waiting for vector to warm up, default 30"
    echo "  --total-samples: the total number of samples to take from vector, default 200"
    echo "  --replicas: the total number of replica experiments to run, default 3"
    echo ""
}

BUILD_IMAGE="true"
REPLICAS=3
TOTAL_SAMPLES=180
WARMUP_SECONDS=30

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
      --tag)
          TAG=$2
          shift # past argument
          shift # past value
          ;;
      --build-image)
          BUILD_IMAGE=$2
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
      --warmup-seconds)
          WARMUP_SECONDS=$2
          shift # past argument
          shift # past value
          ;;
      --total-samples)
          TOTAL_SAMPLES=$2
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
      --replicas)
          REPLICAS=$(($2 - 1))
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

pushd "${__dir}" > /dev/null

IMAGE="vector:${TAG}"
if [[ "$(docker images -q "$IMAGE" 2> /dev/null)" == "" ]]; then
  if [ "${BUILD_IMAGE}" = "true" ]; then
    echo "Image $IMAGE doesn't exist, building"

    ./build_container.sh "${TAG}" "${IMAGE}"
  else
    echo "Image $IMAGE doesn't exist and --build-image was false"
    exit 1
  fi
fi

for ((idx=0; idx <= REPLICAS ; idx++))
do
    SOAK_CAPTURE_DIR="${CAPTURE_DIR}/${SOAK_NAME}/${VARIANT}/${idx}"
    SOAK_CAPTURE_FILE="${SOAK_CAPTURE_DIR}/${VARIANT}.captures"

    pushd "${SOAK_ROOT}/tests/${SOAK_NAME}/" > /dev/null
    echo "[${VARIANT}] Captures will be recorded into ${SOAK_CAPTURE_DIR}"
    mkdir -p "${SOAK_CAPTURE_DIR}"
    touch "${SOAK_CAPTURE_FILE}"
    # shellcheck disable=SC2140
    DOCKER_BUILDKIT=1 docker run --cpus "${SOAK_CPUS}" --memory "${SOAK_MEMORY}" --network "host" --privileged --env RUST_LOG="info" \
                   --mount type=bind,source="${SOAK_ROOT}/tests/${SOAK_NAME}/lading.yaml",target="/etc/lading/lading.yaml",readonly \
                   --mount type=bind,source="${SOAK_ROOT}/tests/${SOAK_NAME}/vector.toml",target="/etc/vector/vector.toml",readonly \
                   --mount type=bind,source="${SOAK_ROOT}/tests/${SOAK_NAME}/data",target="/data",readonly \
                   --mount type=bind,source="${SOAK_CAPTURE_DIR}",target="/tmp/captures" \
                   --user "$(id -u):$(id -g)" \
                   "${IMAGE}" \
                   --config-path "/etc/lading/lading.yaml" \
                   --global-labels "variant=${VARIANT},target=vector,experiment=${SOAK_NAME}" \
                   --capture-path "/tmp/captures/${VARIANT}.captures" \
                   --target-environment-variables "VECTOR_THREADS=${VECTOR_CPUS},VECTOR_LOG=info" \
                   --target-stderr-path /tmp/captures/vector.stderr.log \
                   --target-stdout-path /tmp/captures/vector.stdout.log \
                   --experiment-duration-seconds "${TOTAL_SAMPLES}" \
                   --warmup-duration-seconds "${WARMUP_SECONDS}" \
                   /usr/bin/vector
    popd > /dev/null
done

popd > /dev/null
