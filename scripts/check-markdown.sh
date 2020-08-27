#!/usr/bin/env bash
set -euo pipefail

# check-markdown.sh
#
# SUMMARY
#
#   Checks the markdown format within the Vector repo.
#   This ensures that markdown is consistent and easy to read across the
#   entire Vector repo.

if ! [ -x "$(command -v markdownlint)" ]; then
  echo 'Error: markdownlint is not installed.' >&2
  exit 1
fi

markdownlint \
  --config scripts/.markdownlintrc \
  --ignore scripts/node_modules \
  .
