#!/usr/bin/env bash
set -euo pipefail

# release-homebrew.sh
#
# SUMMARY
#
#   Releases latest version to the vectordotdev homebrew tap

td="$(mktemp -d)"
pushd "$td"

git config --global user.email "vector@datadoghq.com"
git config --global user.name "vic"

git clone "https://$GITHUB_TOKEN:x-oauth-basic@github.com/vectordotdev/homebrew-brew"
cd homebrew-brew

PACKAGE_URL="https://packages.timber.io/vector/$VECTOR_VERSION/vector-$VECTOR_VERSION-x86_64-apple-darwin.tar.gz"
PACKAGE_SHA256=$(curl -fsSL "$PACKAGE_URL" | sha256sum | cut -d " " -f 1)

update-content() {
  sed "s|url \".*\"|url \"$PACKAGE_URL\"|" \
    | sed "s|sha256 \".*\"|sha256 \"$PACKAGE_SHA256\"|" \
    | sed "s|version \".*\"|version \"$VECTOR_VERSION\"|"
}

NEW_CONTENT="$(update-content < Formula/vector.rb)"

echo "$NEW_CONTENT" > Formula/vector.rb

git diff-index --quiet HEAD || git commit -am "Release Vector $VECTOR_VERSION"
git push

popd
rm -rf "$td"
