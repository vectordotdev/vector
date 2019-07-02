#!/usr/bin/env bash

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to Github releases

set -eu

echo "Adding release to Github"
grease create-release timberio/vector $VERSION $CIRCLE_SHA1 --assets "target/artifacts/*"