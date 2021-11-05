#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

display_usage() {
    echo ""
    echo "Usage: $0  CAPTURE_DIR BASELINE_SHA COMPARISON_SHA"
}

CAPTURE_DIR="${1}"
BASELINE_SHA="${2}"
COMPARISON_SHA="${3}"

echo "# Soak Test Results"
echo "Baseline: ${BASELINE_SHA}"
echo "Comparison: ${COMPARISON_SHA}"
echo ""
echo "What follows is a statistical summary of the soak captures between the SHAs given above. Units are bytes/second, except for 'skewness' and 'kurtosis'. Higher numbers in 'comparison' is generally better. Higher skewness or kurtosis numbers indicate a lack of consistency in behavior, making predictions of fitness in the field challenging."
echo ""
for soak_dir in "${CAPTURE_DIR}"/*; do
    SOAK_NAME=$(basename "${soak_dir}")
    echo " --- "
    echo "## \`${SOAK_NAME}\`"
    # NOTE if you change the statistics being pulled here please update the
    # header/body divisor below. Consider that you need one column for the group
    # and one for each statistic.
    OUTPUT=$(
        mlr --itsv --ocsv \
            --from "${soak_dir}/baseline.captures" \
            --from "${soak_dir}/comparison.captures" \
            stats1 -a 'min,p90,p99,max,skewness,kurtosis' -g EXPERIMENT -f VALUE | \
        numfmt --header=1 --to=iec-i --format="%.2f" --field="2-5" --delimiter="," | \
        numfmt --header=1 --to=none  --format="%.2f" --field="6-7" --delimiter=","
    )
    HEADER=$(echo "${OUTPUT}" | head -n1)
    BODY=$(echo "${OUTPUT}" | tail -n+2)

    echo "${HEADER}" | sed 's/,/\ \|\ /g' | sed 's/^/|\ /g' | sed 's/$/\ |/g'
    echo "| --- | --- | --- | --- | --- | --- | --- |"
    echo "${BODY}"   | sed 's/,/\ \|\ /g' | sed 's/^/|\ /g' | sed 's/$/\ |/g'
done
