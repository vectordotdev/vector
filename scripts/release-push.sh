#!/bin/bash
set -euo pipefail

# release-push.rb
#
# SUMMARY
#
#   Pushes new versions produced by `make release` to the repository

cd "$(dirname "${BASH_SOURCE[0]}")/.."
VERSION="$(./scripts/version.sh | sed 's/-nightly$//')"
VERSION_MINOR="$(echo "$VERSION" | grep -o '^[0-9]*\.[0-9]*')"
CURRENT_BRANCH_NAME="$(git branch | awk '{ print $2 }')"

echo "Preparing the branch and the tag..."
set -x
git checkout -b "v$VERSION_MINOR" 2>/dev/null || git checkout "v$VERSION_MINOR"
git merge --ff "$CURRENT_BRANCH_NAME"
git tag -a "v$VERSION" -m "v$VERSION"
set +x

echo "Pushing the branch and the tag..."
set -x
git push origin "v$VERSION_MINOR"
git push origin "v$VERSION"
set +x
