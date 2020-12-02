#!/bin/bash
set -euo pipefail

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to Github releases

grease --debug create-release timberio/vector v${VERSION} ${SHA1} \
  --assets './target/artifacts/*' \
  --notes '[View release notes](${HOST}/releases/${VERSION})' \
  --name v${VERSION}
