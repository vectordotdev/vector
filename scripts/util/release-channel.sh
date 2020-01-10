#!/usr/bin/env bash

# util/release-channel.sh
#
# SUMMARY
#
#   Determines the appropriate release channel (nightly or latest) based
#   on Git HEAD.
#
#   This script is used across various release scripts to determine where
#   distribute archives, packages, etc.

set -eu

if [[ "$(git rev-parse --abbrev-ref HEAD)" == "master" ]]; then
  echo "nightly"
else
  echo "latest"
fi
