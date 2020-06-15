#!/usr/bin/env bash
set -euo pipefail

# check-scripts.sh
#
# SUMMARY
#
#   Checks that scripts pass shellcheck.

FILES=()
while IFS='' read -r LINE; do FILES+=("$LINE"); done < <(git ls-files | grep '\.sh')

shellcheck --shell bash "${FILES[@]}"
