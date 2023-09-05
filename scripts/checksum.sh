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

ARTIFACTS=$(ls | grep -v SHA256SUMS)

sha256sum $ARTIFACTS > vector-$VECTOR_VERSION-SHA256SUMS

popd
