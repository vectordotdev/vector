#!/usr/bin/env bash

# check-generate.sh
#
# SUMMARY
#
#   Checks that there are not any pending documentation changes. This is
#   useful for CI, ensuring that documentation is updated through the
#   /.metadata.toml file instead of the markdown files directly.

set -eu

echo "Checking for pending generation changes..."

changes=$(scripts/generate.rb --dry-run | grep 'Will be changed' || true)

if [[ -n "$changes" ]]; then
  echo 'It looks like the following files would change if `make generate` was run:'
  echo ''
  echo "$changes"
  echo ''
  echo 'This usually means that auto-generated sections were updated. '
  echo 'Instead, you should update the /.metadata.toml file and then run '
  ecgo '`make generate`. See the ./DOCUMENTING.md guide for more info.'
  exit 1
else
  echo 'Nice! No generation changes detected.'
fi
