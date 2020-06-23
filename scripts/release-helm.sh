#!/usr/bin/env bash
set -euo pipefail

# release-helm.sh
#
# SUMMARY
#
#   Package Helm Chart and update the Helm repo.

cd "$(dirname "${BASH_SOURCE[0]}")/.."
set -x

CHANNEL="${CHANNEL:-"$(scripts/util/release-channel.sh)"}"
VERSION="${VERSION:-"$(scripts/version.sh)"}"

if [[ "${USE_TEST_REPO:-"false"}" == "true" ]]; then
  PUBLIC_URL="https://vector-helm-repo-tests.s3.amazonaws.com/helm/$CHANNEL"
  AWS_REPO_URL="s3://vector-helm-repo-tests/helm/$CHANNEL"
else
  PUBLIC_URL="https://packages.timber.io/helm/$CHANNEL"
  AWS_REPO_URL="s3://packages.timber.io/helm/$CHANNEL"
fi

WORKDIR="target/helm"

REPO_DIR="$WORKDIR/repo"
PREVIOUS_MANIFEST="$WORKDIR/previous-manifest.yaml"

# Prepare work directory.
rm -rf "$REPO_DIR"
mkdir -p "$REPO_DIR"

# Package our chart.
helm package \
  distribution/helm/vector \
  --version "$VERSION" \
  --app-version "$VERSION" \
  --destination "$REPO_DIR"

# Download previous manifest.
aws s3 cp "$AWS_REPO_URL/index.yaml" "$PREVIOUS_MANIFEST"

# Update the repo index file.
helm repo index \
  "$REPO_DIR" \
  --merge "$PREVIOUS_MANIFEST" \
  --url "$PUBLIC_URL"

# Upload new files to the repo.
aws s3 cp "$REPO_DIR" "$AWS_REPO_URL" --recursive --sse --acl public-read
