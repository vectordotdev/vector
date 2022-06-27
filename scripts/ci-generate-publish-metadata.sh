#!/usr/bin/env bash
set -euo pipefail

# ci-set-publish-metadata.sh
#
# SUMMARY
#
#   Responsible for setting necessary metadata for our publish workflow in CI.
#
#   Computes the Vector version (from Cargo.toml), the release channel (nightly vs latest``f, which Cloudsmith
#   repository to publish to, and more. All of this information is emitted in a way that sets native outputs on the
#   Github Actions workflow step running the script, which can be passed on to subsequent jobs/steps.

# Generate the Vector version, and build description.
VERSION="${VERSION:-"$(awk -F ' = ' '$1 ~ /^version/ { gsub(/["]/, "", $2); printf("%s",$2) }' Cargo.toml)"}"
echo "::set-output name=vector_version::${VERSION}"

GIT_SHA=$(git rev-parse --short HEAD)
CURRENT_DATE=$(date +%Y-%m-%d)
echo "::set-output name=vector_build_desc::${GIT_SHA} ${CURRENT_DATE}"

# Figure out what our release channel is.
CHANNEL="${CHANNEL:-"$(scripts/release-channel.sh)"}"
echo "::set-output name=vector_release_channel::${CHANNEL}"

# Depending on the channel, this influences which Cloudsmith repository we publish to.
if [[ "${CHANNEL}" == "nightly" ]]; then
	echo "::set-output name=vector_cloudsmith_repo::vector-nightly"
else
	echo "::set-output name=vector_cloudsmith_repo::vector"
fi
