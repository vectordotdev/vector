#!/usr/bin/env bash
set -euo pipefail

GITHUB_OUTPUT="${GITHUB_OUTPUT:-/dev/stdout}"

# ci-set-publish-metadata.sh
#
# SUMMARY
#
#   Responsible for setting necessary metadata for our publish workflow in CI.
#
#   Computes the Vector version (from Cargo.toml), the release channel (nightly vs latest``f, which Cloudsmith
#   repository to publish to, and more. All of this information is emitted in a way that sets native outputs on the
#   GitHub Actions workflow step running the script, which can be passed on to subsequent jobs/steps.

# Generate the Vector version, and build description.
VERSION="${VERSION:-"$(awk -F ' = ' '$1 ~ /^version/ { gsub(/["]/, "", $2); printf("%s",$2) }' Cargo.toml)"}"
echo "vector_version=${VERSION}" >> "${GITHUB_OUTPUT}"

GIT_SHA=$(git rev-parse --short HEAD)
CURRENT_DATE=$(date +%Y-%m-%d)
echo "vector_build_desc=${GIT_SHA} ${CURRENT_DATE}" >> "${GITHUB_OUTPUT}"

# Figure out what our release channel is.
CHANNEL="${CHANNEL:-"$(cargo vdev release channel)"}"
echo "vector_release_channel=${CHANNEL}" >> "${GITHUB_OUTPUT}"

# Depending on the channel, this influences which Cloudsmith repository we publish to.
if [[ "${CHANNEL}" == "nightly" ]]; then
	echo "vector_cloudsmith_repo=vector-nightly" >> "${GITHUB_OUTPUT}"
else
	echo "vector_cloudsmith_repo=vector" >> "${GITHUB_OUTPUT}"
fi
