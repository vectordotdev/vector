#!/bin/bash
set -euo pipefail

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to Github releases

VERSION="${VECTOR_VERSION:-"$(scripts/version.sh)"}"

gh release --repo "vectordotdev/vector" \
  create "v${VERSION}" \
  --title "v${VERSION}" \
  --notes "[View release notes](https://vector.dev/releases/${VERSION}/)" \
  target/artifacts/*
