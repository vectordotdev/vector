#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail
shopt -s globstar

# check-docs.sh
#
# SUMMARY
#
#   Checks that the contents of /docs folder are valid. This includes:
#
#     1. Ensuring the the .cue files can compile.
#     2. Link validation.

DOCS_PATH="docs"

if ! [ -x "$(command -v cue)" ]; then
  echo 'Error: cue is not installed.' >&2
  exit 1
fi

if [[ -z "${CI:-}" ]]; then
  echo "Skipping local formatting - reserved for CI"
else
  echo "Validating ${DOCS_PATH}/**/*.cue formatting."

  cue fmt ${DOCS_PATH}/**/*.cue
  status="$(git status --porcelain ${DOCS_PATH})"

  [[ -z "$status" ]] || {
    echo >&2 "Incorrectly formatted Cue files"
    echo >&2 "$status"
    git diff ${DOCS_PATH}
    exit 1
  }
fi

echo "Validating ${DOCS_PATH}/**/*.cue..."

errors=$(cue vet --concrete --all-errors ${DOCS_PATH}/*.cue ${DOCS_PATH}/**/*.cue)

if [ -n "$errors" ]; then
  printf "Failed!\n\n%s\n" "${errors}"
  exit 1
else
  echo "Success! The contents of the ${DOCS_PATH} directory are valid."
fi
