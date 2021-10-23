#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
set -o xtrace

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
TOTAL_SAMPLES=120

pushd "${__dir}"
./bin/build_container.sh "${SOAK_NAME}" "${BASELINE}"
./bin/build_container.sh "${SOAK_NAME}" "${COMPARISON}"
BASELINE_IMAGE=$(./bin/container_name.sh "${SOAK_NAME}" "${BASELINE}")
COMPARISON_IMAGE=$(./bin/container_name.sh "${SOAK_NAME}" "${COMPARISON}")

capture_dir=$(mktemp --directory /tmp/"${SOAK_NAME}".XXXXXX)
echo "Captures will be recorded into ${capture_dir}"

#
# BASELINE
#
./bin/boot_minikube.sh "${BASELINE_IMAGE}"
minikube mount "${capture_dir}:/captures" &
MOUNT_PID=$!

pushd "${__dir}/${SOAK_NAME}/terraform"
terraform init
terraform apply -var 'type=baseline' -var 'type=baseline' -var "vector_image=${BASELINE_IMAGE}" --auto-approve
echo "Captures will be recorded into ${capture_dir}"
echo "Sleeping for ${WARMUP_GRACE} seconds to allow warm-up"
sleep "${WARMUP_GRACE}"
echo "Recording 'baseline' captures to ${capture_dir}"
sleep "${TOTAL_SAMPLES}"
kill "${MOUNT_PID}"
popd
./bin/shutdown_minikube.sh


#
# COMPARISON
#
./bin/boot_minikube.sh "${COMPARISON_IMAGE}"
minikube mount "${capture_dir}:/captures" &
MOUNT_PID=$!

pushd "${__dir}/${SOAK_NAME}/terraform"
terraform init
terraform apply -var 'type=comparison' -var 'type=comparison' -var "vector_image=${COMPARISON_IMAGE}" --auto-approve
echo "Captures will be recorded into ${capture_dir}"
echo "Sleeping for ${WARMUP_GRACE} seconds to allow warm-up"
sleep "${WARMUP_GRACE}"
echo "Recording 'comparison' captures to ${capture_dir}"
sleep "${TOTAL_SAMPLES}"
kill "${MOUNT_PID}"
popd
./bin/shutdown_minikube.sh

popd

echo "Captures recorded into ${capture_dir}"
echo ""
echo "Here is a statistical summary of that file. Units are bytes."
echo "Higher numbers in the 'comparison' is better."
echo ""
mlr --tsv \
    --from "${capture_dir}/baseline.captures" \
    --from "${capture_dir}/comparison.captures" \
    stats1 -a 'min,p90,p99,max,skewness,kurtosis' -g EXPERIMENT -f VALUE
