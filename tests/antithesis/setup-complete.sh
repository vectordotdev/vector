#!/usr/bin/env bash
set -euo pipefail

# Run this script to inform Antithesis that it can start running test commands.
# You can also use the Antithesis SDK to emit setup-complete from your system if
# that is easier.
#
# Antithesis sets the `ANTITHESIS_OUTPUT_DIR` environment variable
# automatically. This script is setup to emit `setup_complete` to the
# `sdk.jsonl` file in that directory.

OUTPUT_PATH="/tmp/antithesis_sdk.jsonl"
if [[ -n "${ANTITHESIS_OUTPUT_DIR:-}" ]]; then
  OUTPUT_PATH="${ANTITHESIS_OUTPUT_DIR}/sdk.jsonl"
  echo "Running in Antithesis, emitting setup_complete to ${OUTPUT_PATH}"
elif [[ -n "${ANTITHESIS_SDK_LOCAL_OUTPUT:-}" ]]; then
  OUTPUT_PATH="${ANTITHESIS_SDK_LOCAL_OUTPUT}"
  echo "Antithesis SDK local output override detected, emitting setup_complete to ${OUTPUT_PATH}"
fi

mkdir -p "$(dirname "$OUTPUT_PATH")"
echo '{"antithesis_setup":{"status":"complete","details":{"message":"ready to go"}}}' >> "${OUTPUT_PATH}"
