#!/usr/bin/env bash
set -euo pipefail

# check-docs.sh
#
# SUMMARY
#
#   Checks that the contents of the /website/cue folder are valid. This includes:
#
#     1. Ensuring that the .cue files are properly formatted.
#     2. Ensuring that the .cue files can compile.

ROOT=$(git rev-parse --show-toplevel)
CUE="${ROOT}/scripts/cue.sh"

read-all-docs() {
  ${CUE} list | sort | xargs cat -et
}

(
  if ! cue version >/dev/null; then
    echo 'Error: cue is not installed'
    exit 1
  fi

  echo "Validating cue files formatting..."

  # Run formatter (modifies files in-place)
  "${CUE}" fmt

  # Check if any files were modified
  MODIFIED_FILES=$(git diff --name-only "${ROOT}/website/cue")

  if [[ -n "$MODIFIED_FILES" ]]; then
    printf "Incorrectly formatted CUE files:\n\n"
    while IFS= read -r file; do
      echo "  - $file"
    done <<< "$MODIFIED_FILES"
    printf "\nRun './scripts/cue.sh fmt' to fix formatting issues.\n"
    exit 1
  fi

  echo "Validating cue files correctness..."

  if ERRORS="$("${CUE}" vet 2>&1)"; then
    echo "Success! The contents of the sources in the \"./website/cue\" directory are valid"
  else
    printf "Failed!\n\n%s\n" "$ERRORS"
    exit 1
  fi
)
