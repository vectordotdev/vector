#!/usr/bin/env bash

# version.sh
#
# SUMMARY
#
#   Responsible for computing the release version of Vector.
#   This is based on version in Cargo.toml.
#   An optional "nightly" suffix is added if the build channel
#   is nightly.

set -e

VERSION="$(sed -n 's/^version\s=\s"\(.*\)"/\1/p' Cargo.toml)"
CHANNEL="$(scripts/util/release-channel.sh)"
if [ "$CHANNEL" == "nightly" ]; then
  VERSION="$VERSION-nightly"
fi
echo "$VERSION"
