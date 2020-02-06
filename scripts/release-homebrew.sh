#!/usr/bin/env bash

# release-homebrew.sh
#
# SUMMARY
#
#   Releases latest version to the timberio homebrew tap

set -eu

td=$(mktemp -d)
pushd $td

git config --global user.email "bradybot@timber.io"
git config --global user.name "bradybot"

git clone "https://$GITHUB_TOKEN:x-oauth-basic@github.com/timberio/homebrew-brew"
cd homebrew-brew

package_url="https://packages.timber.io/vector/$VERSION/vector-x86_64-apple-darwin.tar.gz"
package_sha256=$(curl -s $package_url | sha256sum | cut -d " " -f 1)

new_content=$(cat Formula/vector.rb | \
  sed "s|url \".*\"|url \"$package_url\"|" | \
  sed "s|sha256 \".*\"|sha256 \"$package_sha256\"|" | \
  sed "s|version \".*\"|version \"$VERSION\"|")

echo "$new_content" > Formula/vector.rb

git commit -am "Release Vector $VERSION"
git push

popd
rm -rf $td
