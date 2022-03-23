#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

display_usage() {
    echo ""
    echo "Usage: soak [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help: display this information"
    echo "  --soak: the experiment to run, default all; space delimited"
    echo "  --build-image: build the soak image if needed, default true"
    echo "  --baseline: the baseline SHA to compare against"
    echo "  --comparison: the SHA to compare against 'baseline'"
    echo "  --cpus: the total number of CPUs to dedicate to the soak minikube, default 7"
    echo "  --memory: the total amount of memory dedicate to the soak minikube, default 8g"
    echo "  --vector-cpus: the total number of CPUs to give to soaked vector, default 4"
    echo "  --warmup-seconds: the total number seconds to pause waiting for vector to warm up, default 30"
    echo "  --total-samples: the total number of samples to take from vector, default 200"
    echo "  --replicas: the total number of replica experiments to run, default 3"
    echo ""
}

BUILD_IMAGE="true"
SOAK_CPUS="7"
SOAK_MEMORY="8g"
VECTOR_CPUS="4"
SOAKS=""
REPLICAS=3
TOTAL_SAMPLES=200
WARMUP_SECONDS=30

while [[ $# -gt 0 ]]; do
  key="$1"

  case $key in
      --soak)
          SOAKS=$2
          shift # past argument
          shift # past value
          ;;
      --baseline)
          BASELINE=$2
          shift # past argument
          shift # past value
          ;;
      --comparison)
          COMPARISON=$2
          shift # past argument
          shift # past value
          ;;
      --build-image)
          BUILD_IMAGE="$2"
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
          echo "unknown option"
          display_usage
          exit 1
          ;;
  esac
done

pushd "${__dir}" > /dev/null

capture_dir=$(mktemp -d /tmp/soak-captures.XXXXXX)
echo "Captures will be recorded into ${capture_dir}"

if [ -z "${SOAKS}" ]; then
    SOAKS=$(ls tests/)
fi

for SOAK_NAME in ${SOAKS}; do
    echo "########"
    echo "########"
    echo "########    ${SOAK_NAME}"
    echo "########"
    echo "########"
    ./bin/soak_one.sh --build-image "${BUILD_IMAGE}" \
                      --soak "${SOAK_NAME}" \
                      --replicas 3 \
                      --variant "baseline" \
                      --tag "${BASELINE}" \
                      --capture-dir "${capture_dir}" \
                      --cpus "${SOAK_CPUS}" \
                      --memory "${SOAK_MEMORY}" \
                      --replicas "${REPLICAS}" \
                      --total-samples "${TOTAL_SAMPLES}" \
                      --warmup-seconds "${WARMUP_SECONDS}" \
                      --vector-cpus "${VECTOR_CPUS}" && \
    ./bin/soak_one.sh --build-image "${BUILD_IMAGE}" \
                      --soak "${SOAK_NAME}" \
                      --replicas 3 \
                      --variant "comparison" \
                      --tag "${COMPARISON}" \
                      --capture-dir "${capture_dir}" \
                      --cpus "${SOAK_CPUS}" \
                      --memory "${SOAK_MEMORY}" \
                      --replicas "${REPLICAS}" \
                      --total-samples "${TOTAL_SAMPLES}" \
                      --warmup-seconds "${WARMUP_SECONDS}" \
                      --vector-cpus "${VECTOR_CPUS}"
done

# Aggregate all captures and analyze them.
./bin/analyze_experiment --capture-dir "${capture_dir}" \
                         --baseline-sha "${BASELINE}" \
                         --comparison-sha "${COMPARISON}" \
                         --vector-cpus "${VECTOR_CPUS}" \
                         --warmup-seconds "${WARMUP_SECONDS}" \
                         --p-value 0.05 # 95% confidence

popd > /dev/null
