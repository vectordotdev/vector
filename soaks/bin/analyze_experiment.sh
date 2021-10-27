#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOAK_ROOT="${__dir}/.."

display_usage() {
    echo ""
    echo "Usage: $0 CAPTURE_DIR"
}

CAPTURE_DIR="${1}"

echo "Captures recorded into ${CAPTURE_DIR}"
echo ""
echo "Here is a statistical summary of the soak captures. Units are bytes,"
echo "except for 'skewness' and 'kurtosis'. Higher numbers in 'comparison'"
echo "is generally better. Higher skewness or kurtosis numbers indicate a"
echo "lack of consistency in behavior, making predictions of fitness in the"
echo "field challenging."
echo ""
mlr --tsv \
    --from "${CAPTURE_DIR}/baseline.captures" \
    --from "${CAPTURE_DIR}/comparison.captures" \
    stats1 -a 'min,p90,p99,max,skewness,kurtosis' -g EXPERIMENT -f VALUE
