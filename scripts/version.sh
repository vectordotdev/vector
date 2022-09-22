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

VERSION="${VERSION:-"$(awk -F ' = ' '$1 ~ /^version/ { gsub(/["]/, "", $2); printf("%s",$2) }' Cargo.toml)"}"
CHANNEL="${CHANNEL:-"$(scripts/release-channel.sh)"}"

if [[ $CHANNEL == "latest" ]] ; then
  TAG="$(git describe --exact-match --tags HEAD)"
  if [[ $TAG != "v$VERSION" ]] ; then
    >&2 echo "On latest release channel and tag, '$TAG', is different from Cargo.toml, '$VERSION'. Aborting"
    exit 1
  fi
fi
echo "$VERSION"
