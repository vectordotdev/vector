#!/usr/bin/env bash

# util/release-channel.sh
#
# SUMMARY
#
#   Determines the appropriate release channel (nightly or latest) based
#   on the current $VERSION.
#
#   This script is used across various release scripts to determine where
#   distribute archives, packages, etc.

set -eu

if [[ $VERSION == *"-"* ]]; then
  echo "nightly"
else
  echo "latest"
fi