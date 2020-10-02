#!/usr/bin/env bash
# shellcheck disable=SC2016
set -uo pipefail

# check-meta.sh
#
# SUMMARY
#
#   Checks that the contents of ./meta are valid according to the
#   /.meta/.schema.json schema definition.

META_PATH="/.meta"

echo "Validating ${META_PATH}..."

if ! [ -x "$(command -v jsonschema)" ]; then
  echo 'Error: jsonschema is not installed.' >&2
  exit 1
fi

td="$(mktemp -d)"
scripts/load-meta.rb > "$td/meta.json"
errors=$(jsonschema -i "$td/meta.json" -F "* Message: {error.message}
  Path: {error.path}

" .meta/.schema.json 2>&1)

if [ -n "$errors" ]; then
  echo "Failed! ${errors}"
  exit 1
else
  echo "Success! The contents of the ./meta directory are valid."
fi

rm -rf "$td"
