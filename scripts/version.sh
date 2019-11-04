#!/usr/bin/env bash

# version.sh
#
# SUMMARY
#
#   Responsible for computing the release version of Vector.
#   This is based on version in Cargo.toml.
#   An optional `nightly` suffix is added if NIGHTLY environment
#   variable set to 1.

set -e

VERSION="$(sed -n 's/^version\s= "\(.*\)"/\1/p' Cargo.toml)"
if [ "$NIGHTLY" == 1 ]; then
  VERSION="$VERSION-nightly"
fi
echo "$VERSION"