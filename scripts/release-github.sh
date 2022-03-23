#!/bin/bash
set -euo pipefail

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to Github releases

VERSION="${VECTOR_VERSION:-"$(scripts/version.sh)"}"

grease --debug create-release vectordotdev/vector "v${VERSION}" "${SHA1}" \
  --assets './target/artifacts/*' \
  --notes "[View release notes](https://vector.dev/releases/${VERSION}/)" \
  --name "v${VERSION}"
