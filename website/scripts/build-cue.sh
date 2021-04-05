#!/usr/bin/env bash

cue version

ROOT="$(git rev-parse --show-toplevel)"
DOCS_DIR="${ROOT}/docs"
SITE_DIR="${ROOT}/website"
DATA_DIR="${SITE_DIR}/data"
JSON_OUT="${DATA_DIR}/docs.json"

mkdir -p "${DATA_DIR}"

rm "${JSON_OUT}" 2> /dev/null

find "${DOCS_DIR}" -name "*.cue" | xargs cue export --all-errors "$@" --outfile "${JSON_OUT}"
