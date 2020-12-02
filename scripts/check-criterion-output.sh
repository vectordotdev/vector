#!/usr/bin/env bash
set -euo pipefail

# check-criterion-output.sh
#
# SUMMARY
#
#   Used to reformat benchmark output from criterion into a condensed format. It
#   reads from stdin and writes to stdout.

DIR="$(dirname "${BASH_SOURCE[0]}")"

awk --file "$DIR/parse-criterion-output.awk" |
  jq --slurp --raw-output '.[] | [.name, .time, .time_change, .throughput, .throughput_change, .change] | @tsv' |
  column --separator $'\t' --table --table-columns "name,time,time change,throughput,throughput change,change" |
  awk -v rc=0 '/regressed/ { rc=1 } 1; END { if (rc == 1) { print "\nRegression detected. Note that any regressions should be verified."; exit rc }}'
