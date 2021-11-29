#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
# set -o xtrace

display_usage() {
    echo ""
    echo "Usage: analyze_experiment [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help: display this information"
    echo "  --capture-dir: the directory in which to write captures"
    echo "  --baseline: the baseline SHA to compare against"
    echo "  --comparison: the SHA to compare against 'baseline'"
    echo "  --vector-cpus: the total number of CPUs to give to soaked vector, default 4"
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
      --baseline)
          BASELINE_SHA=$2
          shift # past argument
          shift # past value
          ;;
      --comparison)
          COMPARISON_SHA=$2
          shift # past argument
          shift # past value
          ;;
      --vector-cpus)
          VECTOR_CPUS=$2
          shift # past argument
          shift # past value
          ;;
      --capture-dir)
          CAPTURE_DIR=$2
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

echo "# Soak Test Results"
echo "Baseline: ${BASELINE_SHA}"
echo "Comparison: ${COMPARISON_SHA}"
echo "Total Vector CPUs: ${VECTOR_CPUS}"
echo ""
echo "What follows is a statistical summary of the soak captures between the SHAs given above. Units are bytes/second/CPU, except for 'skewness' and 'kurtosis'. Higher numbers in 'comparison' is generally better. Higher skewness or kurtosis numbers indicate a lack of consistency in behavior, making predictions of fitness in the field challenging."
echo ""
echo "<details>"
echo "  <summary>Click to expand!</summary>"
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
            stats1 -a 'min,p90,p99,max,skewness,kurtosis' -g EXPERIMENT -f VALUE
    )
    HEADER=$(echo "${OUTPUT}" | head -n1)
    BODY=$(echo "${OUTPUT}" | tail -n+2 | \
           awk -v cpus="${VECTOR_CPUS}" 'BEGIN {FS=",";OFS=",";OFMT="%f"} {print $1,$2/cpus,$3/cpus,$4/cpus,$5/cpus,$6,$7}' | \
           numfmt --to=iec-i --format="%.2f" --field="2-5" --delimiter="," | \
           numfmt --to=none  --format="%.2f" --field="6-7" --delimiter=","
    )

    echo "${HEADER}" | sed 's/,/\ \|\ /g' | sed 's/^/|\ /g' | sed 's/$/\ |/g'
    echo "| --- | --- | --- | --- | --- | --- | --- |"
    echo "${BODY}"   | sed 's/,/\ \|\ /g' | sed 's/^/|\ /g' | sed 's/$/\ |/g'
done
echo "</details>"
