#!/usr/bin/env bash

# release-channel.sh
#
# SUMMARY
#
#   Determines the appropriate release channel (nightly or latest) based
#   on Git HEAD.
#
#   This script is used across various release scripts to determine where
#   distribute archives, packages, etc.

set -eu

# if this is a git tag, assume a real release
if git describe --exact-match --tags HEAD 2> /dev/null ; then
  echo "latest"
else
  echo "nightly"
fi
