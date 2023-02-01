#!/bin/bash
set -euo pipefail

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to GitHub releases

VERSION="${VECTOR_VERSION:-"$(cargo vdev version)"}"

gh release --repo "vectordotdev/vector" \
  create "v${VERSION}" \
  --title "v${VERSION}" \
  --notes "[View release notes](https://vector.dev/releases/${VERSION}/)" \
  target/artifacts/*
