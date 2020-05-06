#!/usr/bin/env bash
set -euo pipefail

# version.sh
#
# SUMMARY
#
#   Responsible for computing the release version of Vector.
#   This is based on version in Cargo.toml.
#   An optional "nightly" suffix is added if the build channel
#   is nightly.

VERSION="${VERSION:-"$(sed -n 's/^version\s=\s"\(.*\)"/\1/p' Cargo.toml)"}"
CHANNEL="${CHANNEL:-"$(scripts/util/release-channel.sh)"}"
if [ "$CHANNEL" == "nightly" ]; then
  VERSION="$VERSION-nightly"
fi
echo "$VERSION"
