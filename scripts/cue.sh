#!/usr/bin/env bash
set -euo pipefail

# cue.sh
#
# SUMMARY
#
#   CUE utilities.

ROOT=$(git rev-parse --show-toplevel)
CUE_SOURCES="${ROOT}/website/cue"

list-docs-files() {
  find "${CUE_SOURCES}" -name '*.cue'
}

cmd_list() {
  list-docs-files
}

cmd_fmt() {
  list-docs-files | xargs cue fmt "$@"
}

cmd_vet() {
  list-docs-files | xargs cue vet --concrete --all-errors "$@"
}

cmd_eval() {
  list-docs-files | xargs cue eval --concrete --all-errors "$@"
}

cmd_export() {
  list-docs-files | xargs cue export --all-errors "$@"
}

usage() {
  cat >&2 <<-EOF
Usage: $0 MODE

Modes:
  list    - list all the documentation files
  fmt     - format all the documentation files
  vet     - check the documentation files and print errors
  eval    - print the evaluated documentation,
            optionally pass the expression to evaluate via "-e EXPRESSION"
  export  - export the documentation,
            optionally pass the expression to evaluate via "-e EXPRESSION"

Examples:

  Print the whole documentation in the JSON format:

    $0 export

  Print the "components.sources.kubernetes_logs" subtree in CUE format:

    $0 eval -e components.sources.kubernetes_logs

  Print the "cli" subtree in JSON format:

    $0 export -e cli

EOF
  exit 1
}

MODE="${1:-}"
case "$MODE" in
  list|fmt|vet|eval|export)
    shift
    "cmd_$MODE" "$@"
    ;;
  *)
    usage
    ;;
esac
