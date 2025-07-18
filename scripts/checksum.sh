#!/usr/bin/env bash
set -euo pipefail

# checksum.sh
#
# SUMMARY
#
#   Creates a SHA256 checksum of all artifacts created during CI

ROOT=$(git rev-parse --show-toplevel)
VECTOR_VERSION=${VECTOR_VERSION:-nightly}

pushd "${ROOT}/target/artifacts"

shopt -s extglob
ARTIFACTS=(!(*SHA256SUMS))
shopt -u extglob

sha256sum "${ARTIFACTS[@]}" > vector-"$VECTOR_VERSION"-SHA256SUMS

popd
