#!/usr/bin/env bash
set -euo pipefail

# check-docs.sh
#
# SUMMARY
#
#   Checks that the contents of the /website/cue folder are valid. This includes:
#
#     1. Ensuring that the .cue files can compile.
#     2. In CI, ensuring that the .cue files are properly formatted.

ROOT=$(git rev-parse --show-toplevel)
CUE="${ROOT}/website/scripts/cue.sh"

read-all-docs() {
  ${CUE} list | sort | xargs cat -et
}

(
  if ! cue version >/dev/null; then
    echo 'Error: cue is not installed'
    exit 1
  fi

  if [[ "${CI:-"false"}" != "true" ]]; then
    echo "Skipping cue files format validation - reserved for CI"
  else
    echo "Validating cue files formatting..."

    STATE_BEFORE="$(read-all-docs)"
    "${CUE}" fmt
    STATE_AFTER="$(read-all-docs)"

    if [[ "$STATE_BEFORE" != "$STATE_AFTER" ]]; then
      printf "Incorrectly formatted CUE files\n\n"
      diff --unified <(echo "$STATE_BEFORE") <(echo "$STATE_AFTER")
      exit 1
    fi
  fi

  echo "Validating cue files correctness..."

  if ERRORS="$("${CUE}" vet 2>&1)"; then
    echo "Success! The contents of the sources in the \"./website/cue\" directory are valid"
  else
    printf "Failed!\n\n%s\n" "$ERRORS"
    exit 1
  fi
)
