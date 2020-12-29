#!/usr/bin/env bash
set -euo pipefail

# check-criterion-output.sh
#
# SUMMARY
#
#   Used to reformat benchmark output from criterion into a condensed format. It
#   reads from stdin and writes to stdout.

DIR="$(dirname "${BASH_SOURCE[0]}")"

# Always exit 0 until we resolve
# https://github.com/timberio/vector/issues/5394
(
  echo -e "name\ttime\ttime change\tthroughput\tthroughput change\tchange";
  awk --file "$DIR/parse-criterion-output.awk" |
    jq --slurp --raw-output '.[] | [.name, .time, .time_change, .throughput, .throughput_change, .change] | @tsv'
) |
  column -s $'\t' -t  |
  (awk -v rc=0 '/regressed/ { rc=1 } 1; END { if (rc == 1) { print "\nRegression detected. Note that any regressions should be verified."; exit rc }}' || true)
