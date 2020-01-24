#!/bin/bash

# release-push.rb
#
# SUMMARY
#
#   Pushes new versions produced by `make release` to the repository

set -euo pipefail

if ! [ -x "$(command -v netlify)" ]; then
  error 'Error: netlify is not installed. (npm install netlify-cli -g)' >&2
  exit 1
fi

cd $(dirname $0)/..
version=$(./scripts/version.sh | sed 's/-nightly$//')
version_minor=$(echo $version | grep -o '^[0-9]*\.[0-9]*')
current_branch_name=$(git branch | awk '{ print $2 }')
netlify_site_id="abeaffe6-d38a-4f03-8b6c-c6909e94918e"

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

echo "Updating Netlify to point to the v$version_minor branch"
netlify login
netlify api updateSite --data '{ "site_id": "$netlify_site_id", "repo": { "repo_branch": "v$version_minor" } }'
netlify api createSiteDeploy --data '{ "site_id": "$netlify_site_id" }'