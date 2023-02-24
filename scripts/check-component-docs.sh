#!/usr/bin/env bash
set -euo pipefail

# check-component-docs.sh
#
# SUMMARY
#
#   Checks that there are no changed machine-generated component Cue files after running the
#   generation step via `make generate-component-docs`.
#
#   This should only be run in CI, after calling `make generate-component-docs`, as it depends on
#   the presence of dirty files in the working directory to detect if the machine-generated files
#   are out-of-date.

DIRTY_COMPONENT_FILES=$(git ls-files --full-name --modified --others --exclude-standard | { grep website/cue/reference/components || test $? = 1; } | sed 's/^/  - /g')
if [[ -n "${DIRTY_COMPONENT_FILES}" ]]; then
   echo "Found out-of-sync component Cue files in this branch:"
   echo "${DIRTY_COMPONENT_FILES}"
   echo
   echo "Run \`make generate-component-docs\` locally to update your branch and commit/push the changes."
   exit 1
fi
