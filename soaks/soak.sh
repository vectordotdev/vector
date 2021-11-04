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
    echo "  --soak: the experiment to run"
    echo "  --remote-image|--local-image: whether to use a local vector image or remote"
    echo "  --baseline: the baseline SHA to compare against"
    echo "  --comparison: the SHA to compare against 'baseline'"
    echo ""
}

USE_LOCAL_IMAGE="true"

while [[ $# -gt 0 ]]; do
  key="$1"

  case $key in
      --soak)
          SOAK_NAME=$2
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
      --remote-image)
          USE_LOCAL_IMAGE="false"
          shift # past argument
          ;;
      --local-image)
          USE_LOCAL_IMAGE="true"
          shift # past argument
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

pushd "${__dir}"

capture_dir=$(mktemp -d /tmp/"${SOAK_NAME}".XXXXXX)
echo "Captures will be recorded into ${capture_dir}"

./bin/soak_one.sh "${USE_LOCAL_IMAGE}" "${SOAK_NAME}" "baseline" "${BASELINE}" "${capture_dir}"
./bin/soak_one.sh "${USE_LOCAL_IMAGE}" "${SOAK_NAME}" "comparison" "${COMPARISON}" "${capture_dir}"

popd

./bin/analyze_experiment.sh "${capture_dir}" "${BASELINE}" "${COMPARISON}"
