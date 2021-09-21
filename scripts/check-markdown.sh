#!/usr/bin/env bash
set -euo pipefail

# check-markdown.sh
#
# SUMMARY
#
#   Checks the markdown format within the Vector repo.
#   This ensures that markdown is consistent and easy to read across the
#   entire Vector repo.

markdownlint \
  --config scripts/.markdownlintrc \
  --ignore scripts/node_modules \
  --ignore website/node_modules \
  --ignore target \
  .
