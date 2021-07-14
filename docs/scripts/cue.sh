#!/usr/bin/env bash
set -euo pipefail

# cue.sh
#
# SUMMARY
#
#   CUE utilities.

ROOT=$(git rev-parse --show-toplevel)
CUE_SOURCES="${ROOT}/docs/cue"
JSON_OUT="${ROOT}/docs/data/docs.json"
CHECK_DOCS_SCRIPT="${ROOT}/scripts/check-docs.sh"

list-docs-files() {
  find "${CUE_SOURCES}" -name '*.cue'
}

cmd_check() {
  ${CHECK_DOCS_SCRIPT}
}

cmd_format() {
  list-docs-files | xargs cue fmt "$@"
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

cmd_build() {
  # Display the CUE version for CI debugging purposes
  cue version

  # The docs JSON file needs to be removed or else CUE errors
  rm -f "${JSON_OUT}"

  # Build the docs JSON object out of the CUE sources
  list-docs-files | xargs cue export --all-errors "$@" --outfile "${JSON_OUT}"
}

usage() {
  cat >&2 <<-EOF
Usage: $0 MODE

Modes:
  check   - check for the CUE sources' correctness
  format  - format all CUE files using the built-in formatter
  list    - list all the documentation files
  fmt     - format all the documentation files
  vet     - check the documentation files and print errors
  eval    - print the evaluated documentation,
            optionally pass the expression to evaluate via "-e EXPRESSION"
  build   - build all of the CUE sources and export them into a Hugo-processable
            JSON object

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
  check|format|list|fmt|vet|eval|build)
    shift
    "cmd_$MODE" "$@"
    ;;
  *)
    usage
    ;;
esac
