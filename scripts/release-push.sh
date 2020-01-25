#!/bin/bash

# release-push.rb
#
# SUMMARY
#
#   Pushes new versions produced by `make release` to the repository

set -euo pipefail

cd $(dirname $0)/..
version=$(./scripts/version.sh | sed 's/-nightly$//')
version_minor=$(echo $version | grep -o '^[0-9]*\.[0-9]*')
current_branch_name=$(git branch | awk '{ print $2 }')

echo "Preparing the branch and the tag..."
set -x
git checkout -b v$version_minor 2>/dev/null || git checkout v$version_minor
git merge --ff $current_branch_name
git tag v$version
set +x

echo "Pushing the branch and the tag..."
set -x
git push origin v$version_minor
git push origin v$version
set +x

echo "Would you overwrite the `latest` branch with this version?"
echo "Note: this will update the website to point to this branch"
print "(y/n) "
read update_website

if [ $update_website = "y" ]; then
  echo "Updating the `latest` branch to reflect this version..."
  set -x
  git checkout v$version_minor
  git merge -s ours latest
  git push origin latest --force
  git checkout $current_branch_name
fi