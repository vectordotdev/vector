#!/usr/bin/env bash

CUE_DIR="cue"
DATA_DIR="data"
JSON_OUT="${DATA_DIR}/docs.json"

# Display the CUE version for CI debugging purposes
cue version

# The docs JSON file needs to be removed or else CUE errors
rm "${JSON_OUT}" 2> /dev/null

# Build the docs JSON object out of the CUE sources
find "${CUE_DIR}" -name "*.cue" -print0 | xargs -0 cue export --all-errors "$@" --outfile "${JSON_OUT}"
