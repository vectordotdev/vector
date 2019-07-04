#!/usr/bin/env bash

# check-docs.sh
#
# SUMMARY
#
#   Checks that there are not any pending documentation changes. This is
#   useful for CI, ensuring that documentation is updated through the
#   /.metadata.toml file instead of the markdown files directly.

set -eu

echo "Checking for pending documention changes..."

changes=$(scripts/generate-docs.sh --dry-run | grep 'Will be changed' || true)

if [[ -n "$changes" ]]; then
  echo 'It looks like the following files would change if `make generate-docs` was run:'
  echo ''
  echo "$changes"
  echo ''
  echo 'This usually means that auto-generated sections in the documentation'
  echo 'were updated. Instead, you should update the /.metadata.toml file and'
  echo 'then run `make generate-docs`. See the ./DOCUMENTING.md guide for more'
  echo 'info.'
  exit 1
else
  echo 'Nice! No documentation changes detected.'
fi
