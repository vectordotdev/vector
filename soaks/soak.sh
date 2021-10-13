#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

display_usage() {
	echo -e "\nUsage: \$0 SOAK_NAME BASELINE_SHA COMPARISON_SHA\n"
}

if [  $# -le 1 ]
then
    display_usage
    exit 1
fi

SOAK_NAME="${1:-}"
BASELINE="${2:-}"
COMPARISON="${3:-}"
WARMUP_GRACE=90
TOTAL_SAMPLES=300

collect_samples() {
    local PROM_URL
    PROM_URL=$(minikube service --url --namespace monitoring prometheus)
    local EXPERIMENT_TYPE="${1}"
    local CAPTURE_FILE="${2}"

    local sample_idx=0
    while [ $sample_idx -ne $TOTAL_SAMPLES ]
    do
        SAMPLE=$(curl --silent ${PROM_URL}/api/v1/query\?query\="sum(rate((bytes_written\[1m\])))" | jq '.data.result[0].value[1]' | sed 's/"//g')
        echo -e "${EXPERIMENT_TYPE}\t${sample_idx}\t${SAMPLE}" >> "${CAPTURE_FILE}"
        sleep 1
        sample_idx=$(($sample_idx+1))
    done
}


pushd "${__dir}"
BASELINE_IMAGE=$(./bin/build.sh "${SOAK_NAME}" "${BASELINE}")
COMPARISON_IMAGE=$(./bin/build.sh "${SOAK_NAME}" "${COMPARISON}")

capture_file=$(mktemp /tmp/"${SOAK_NAME}"-captures.XXXXXX)
echo "Captures will be recorded to ${capture_file}"
echo -e "EXPERIMENT\tSAMPLE-IDX\tSAMPLE" > "${capture_file}"

./bin/boot_minikube.sh "${BASELINE_IMAGE}" "${COMPARISON_IMAGE}"
pushd "${__dir}/${SOAK_NAME}/terraform"

terraform init
#
# BASELINE
#
terraform apply -var 'type=baseline' -var 'type=baseline' -var "vector_image=${BASELINE_IMAGE}" -var "sha=${BASELINE}" --auto-approve
sleep "${WARMUP_GRACE}"
echo "Recording 'baseline' captures to ${capture_file}"
collect_samples "baseline" "${capture_file}"
terraform apply -var 'type=baseline' -var "vector_image=${BASELINE_IMAGE}" -var "sha=${BASELINE}" --destroy --auto-approve

#
# COMPARISON
#
terraform apply -var 'type=comparison' -var "vector_image=${COMPARISON_IMAGE}" -var "sha=${COMPARISON}" --auto-approve
sleep "${WARMUP_GRACE}"
echo "Recording 'comparison' captures to ${capture_file}"
collect_samples "comparision" "${capture_file}"

popd
./bin/shutdown_minikube.sh

popd
echo "Captures recorded to ${capture_file}"
echo ""
echo "Here is a statistical summary of that file. Units are bytes."
echo "Higher numbers in the 'comparision' is better."
echo ""
mlr --tsv --from "${capture_file}" stats1 -a 'min,p90,p99,max,skewness,kurtosis' -g EXPERIMENT -f SAMPLE | column -t
