#!/usr/bin/env bash

# util/release-channel.sh
#
# SUMMARY
#
#   Determines the appropriate release channel (nightly or latest) based
#   on presence of $NIGHTLY environment variable.
#
#   This script is used across various release scripts to determine where
#   distribute archives, packages, etc.

set -eu

if [[ "$NIGHTLY" == "1" ]]; then
  echo "nightly"
else
  echo "latest"
fi
