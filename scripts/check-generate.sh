#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail

# check-generate.sh
#
# SUMMARY
#
#   Checks that there are not any pending documentation changes. This is
#   useful for CI, ensuring that documentation is updated through the
#   /.meta dir instead of the markdown files directly.

echo "Checking for pending generation changes..."

CHANGES="$(scripts/generate.rb --dry-run | grep 'Will be changed' || true)"
CHANGES_EXIT_CODE="$?"

if [[ "$CHANGES_EXIT_CODE" != "0" ]]; then
  echo 'The `scripts/generate.rb --dry-run` command returned an error:'
  echo ''
  echo "$CHANGES"
  echo ''
  exit 1
fi

if [[ -n "$CHANGES" ]]; then
  echo 'It looks like the following files would change if `make generate` was run:'
  echo ''
  echo "$CHANGES"
  echo ''
  echo 'This usually means that auto-generated sections were updated. '
  echo 'Instead, you should update the files in the /.meta dir and then run '
  echo '`make generate`. See the ./DOCUMENTING.md guide for more info.'
  exit 1
else
  echo 'Nice! No generation changes detected.'
fi
