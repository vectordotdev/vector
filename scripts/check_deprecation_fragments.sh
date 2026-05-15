#!/usr/bin/env bash

# Validates all deprecation.d/ fragment files for correct format.
# Run locally at any time; run in CI on PRs that touch deprecation.d/.

DEPRECATION_DIR="deprecation.d"
VERSION_RE='^[0-9]+\.[0-9]+(\.[0-9]+)?$'

if [ ! -d "${DEPRECATION_DIR}" ]; then
  echo "No ./${DEPRECATION_DIR} found. This tool must be invoked from the root of the repo."
  exit 1
fi

FRAGMENTS=$(find "${DEPRECATION_DIR}" -maxdepth 1 -name "*.md" ! -name "README.md" | sort)

if [ -z "$FRAGMENTS" ]; then
  echo "No deprecation fragments found in ${DEPRECATION_DIR}/."
  exit 0
fi

error=0

while IFS= read -r fpath; do
  fname=$(basename "$fpath")
  echo "validating '${fname}'"

  # Must end with .md (guaranteed by find, but be explicit)
  if [[ "${fname}" != *.md ]]; then
    echo "  error: file must have a .md extension (${fname})"
    error=1
    continue
  fi

  # Must start with opening ---
  first_line=$(head -n 1 "$fpath")
  if [[ "${first_line}" != "---" ]]; then
    echo "  error: file must begin with YAML frontmatter (---) (${fname})"
    error=1
    continue
  fi

  # Must have a closing ---
  if ! awk 'NR>1 && /^---$/ { found=1; exit } END { exit !found }' "$fpath"; then
    echo "  error: file has unclosed frontmatter — missing closing '---' (${fname})"
    error=1
    continue
  fi

  # Extract frontmatter block (between the two ---)
  frontmatter=$(awk '/^---$/ { if (++n == 2) exit; next } n == 1' "$fpath")

  # what: is required and must be non-empty
  what_line=$(echo "$frontmatter" | grep -E '^what:')
  if [ -z "$what_line" ]; then
    echo "  error: missing required field 'what' (${fname})"
    error=1
    continue
  fi
  what_value=$(echo "$what_line" | sed 's/^what:[[:space:]]*//' | tr -d '"'"'" | tr -d '[:space:]')
  if [ -z "$what_value" ]; then
    echo "  error: 'what' field must not be empty (${fname})"
    error=1
    continue
  fi

  # deprecation_version: is required; value must be TBD or a semver string
  dep_line=$(echo "$frontmatter" | grep -E '^deprecation_version:')
  if [ -z "$dep_line" ]; then
    echo "  error: missing required field 'deprecation_version' (${fname})"
    error=1
    continue
  fi
  dep_value=$(echo "$dep_line" | sed 's/^deprecation_version:[[:space:]]*//' | tr -d '"'"'")
  if [[ "${dep_value}" != "TBD" ]] && ! [[ "${dep_value}" =~ $VERSION_RE ]]; then
    echo "  error: 'deprecation_version' must be \"TBD\" or a version like \"0.56\" or \"0.56.0\" (got '${dep_value}') (${fname})"
    error=1
    continue
  fi

  # announcement_version: optional; if present must be TBD or a semver string
  ann_line=$(echo "$frontmatter" | grep -E '^announcement_version:')
  if [ -n "$ann_line" ]; then
    ann_value=$(echo "$ann_line" | sed 's/^announcement_version:[[:space:]]*//' | tr -d '"'"'")
    if [[ "${ann_value}" != "TBD" ]] && ! [[ "${ann_value}" =~ $VERSION_RE ]]; then
      echo "  error: 'announcement_version' must be \"TBD\" or a version like \"0.56\" or \"0.56.0\" (got '${ann_value}') (${fname})"
      error=1
      continue
    fi
  fi

done <<< "$FRAGMENTS"

if [ "$error" -ne 0 ]; then
  exit 1
fi

echo "deprecation fragments are valid."
