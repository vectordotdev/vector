#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

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
    echo "  --warmup-seconds: the total number seconds to pause waiting for vector to warm up"
    echo ""
}

BUILD_IMAGE="true"

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

pushd "${__dir}"


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

./run_experiment.sh --capture-dir "${CAPTURE_DIR}" \
                    --variant "${VARIANT}" \
                    --image "${IMAGE}" \
                    --soak "${SOAK_NAME}" \
                    --cpus "${SOAK_CPUS}" \
                    --memory "${SOAK_MEMORY}" \
                    --vector-cpus "${VECTOR_CPUS}" \
                    --warmup-seconds "${WARMUP_SECONDS}"

popd
