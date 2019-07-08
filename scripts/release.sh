#!/usr/bin/env bash

# release.sh
#
# SUMMARY
#
#   A script that handles all of the steps to trigger an release via CI:
#
#   1. Updates the CHANGELOG.md to remove the `-dev` mention.
#   2. Commits the change as "Release vX.X.X"
#   3. Tags the commit with vX.X.X
#   4. Creates a new branch for the new minor branch
#   5. Updates the CHANGELOG.md again with the new version. Ex: vX.X.X-dev
#   6. Commits the CHANGELOG.md change
#   7. Pushes everything to origin

#
# Check
# Perform various checks to ensure we are not releasing in a bad state.
#

set -eu

current_branch=$(git branch | grep \* | cut -d ' ' -f2)

if [[ "$current_branch" != "master" ]]; then
  echo "You must be on the master branch"
  exit 1
fi

has_unstaged_changes=$(git diff-index --name-only HEAD --)

if [[ -n "$has_unstaged_changes" ]]; then
  echo "You have unstaged changes, please commit them first"
  exit 1
fi

#
# Determine versions
# Collect version information and verify that it is correct.
#

previous_version_minor=$(grep -o -E 'v.*\.X' CHANGELOG.md | head -1 | sed 's/^v//g' | sed 's/\.X$//g')
current_version=$(grep -o -E 'v.*-dev' CHANGELOG.md | head -1 | sed 's/^v//g' | sed 's/-dev$//g')
current_version_minor=$(echo "$current_version" | cut -d. -f-2)

echo "Current version detected: $current_version"
echo -n "What's the next version? (ex: 0.4.0 or 1.0.0) "

read new_version
new_version_minor=$(echo "$new_version" | cut -d. -f-2)

echo ""
echo "Previous minor version: $previous_version_minor"
echo "Version to be released: $current_version"
echo "Next version: $new_version"
echo ""

echo -n "Does this look right? (y/n) "

while true; do
  read _choice
  case $_choice in
    y) break; ;;
    n) exit; ;;
    *) echo "Please enter y or n"; ;;
  esac
done

#
# Current version
# Make changes necessary to lock in the new version
#

_changelog=$(cat CHANGELOG.md | sed "s/$current_version-dev/$current_version/g")
echo "$_changelog" > CHANGELOG.md

escaped_current_version=$(echo $current_version | sed "s/\./\\\./g")
_cargo=$(cat Cargo.toml | sed "1,/version = \"$escaped_current_version-dev\"/ s/version = \"$escaped_current_version-dev\"/version = \"$escaped_current_version\"/")
echo "$_cargo" > Cargo.toml
cargo check

git commit -am "Release v$current_version"
git tag -a v$current_version -m "v$current_version"
git checkout -b v$current_version_minor
git checkout master

#
# New version
# Bump to the new -dev version.
#

echo "
# Changelog for Vector v$new_version-dev

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## v$new_version-dev

### Added

### Changed

### Deprecated

### Fixed

### Removed

### Security

## v$current_version_minor.X

The CHANGELOG for v$current_version_minor.X releases can be found in the [v$current_version_minor branch](https://github.com/timberio/vector/blob/v$current_version_minor/CHANGELOG.md).
" > CHANGELOG.md

escaped_current_version=$(echo $current_version | sed "s/\./\\\./g")
escaped_new_version=$(echo $new_version | sed "s/\./\\\./g")
_cargo=$(cat Cargo.toml | sed "1,/version = \"$escaped_current_version\"/ s/version = \"$escaped_current_version\"/version = \"$escaped_new_version-dev\"/")
echo "$_cargo" > Cargo.toml
cargo check

git commit -am "Start v$new_version-dev"

#
# Push
#

git push origin
git push -u origin v$current_version_minor
git push origin v$current_version
