#!/usr/bin/env bash
set -euo pipefail

# check-docs.sh
#
# SUMMARY
#
#   Checks that the contents of /docs folder are valid. This includes:
#
#     1. Ensuring the the .cue files can compile.
#     2. In CI, ensuring the the .cue files are properly formatted.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

read-all-docs() {
  scripts/cue.sh list | sort | xargs cat -A
}

if ! cue version >/dev/null; then
  echo 'Error: cue is not installed'
  exit 1
fi

if [[ "${CI:-"false"}" != "true" ]]; then
  echo "Skipping cue files format validation - reserved for CI"
else
  echo "Validating cue files formatting..."

  STATE_BEFORE="$(read-all-docs)"
  scripts/cue.sh fmt
  STATE_AFTER="$(read-all-docs)"

  if [[ "$STATE_BEFORE" != "$STATE_AFTER" ]]; then
    printf "Incorrectly formatted CUE files\n\n"
    diff --unified <(echo "$STATE_BEFORE") <(echo "$STATE_AFTER")
    exit 1
  fi
fi

echo "Validating cue files correctness..."

if ERRORS="$(scripts/cue.sh vet 2>&1)"; then
  echo "Success! The contents of the \"docs/\" directory are valid"
else
  printf "Failed!\n\n%s\n" "$ERRORS"
  exit 1
fi
