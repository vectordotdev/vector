#!/usr/bin/env bash

set -e

git_tag=$(git describe --exact-match --tags HEAD 2> /dev/null || echo "")

if [ -z "${git_tag}" ]
then
  git describe --tags | sed 's/^v//g'
else
  git describe --abbrev=0 --tags | sed 's/^v//g'
fi