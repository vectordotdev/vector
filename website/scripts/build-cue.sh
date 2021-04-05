#!/usr/bin/env bash

ROOT="$(git rev-parse --show-toplevel)"
DOCS_DIR="${ROOT}/docs"
SITE_DIR="${ROOT}/website"
DATA_DIR="${SITE_DIR}/data"
JSON_OUT="${DATA_DIR}/docs.json"

# Display the CUE version for CI debugging purposes
cue version

# The docs JSON file needs to be removed or else CUE errors
rm "${JSON_OUT}" 2> /dev/null

# Build the docs JSON object out of the CUE sources
find "${DOCS_DIR}" -name "*.cue" | xargs cue export --all-errors "$@" --outfile "${JSON_OUT}"
