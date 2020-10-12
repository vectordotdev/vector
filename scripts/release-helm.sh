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

if [[ "$CHANNEL" == "nightly" ]]; then
  DATE="${DATE:-"$(date -u +%Y-%m-%d)"}"
  APP_VERSION="nightly-$DATE" # matches the version part of the image tag
  CHART_VERSION="$VERSION-$DATE"
else
  APP_VERSION="$VERSION" # matches the version part of the image tag
  CHART_VERSION="$VERSION"
fi

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

capture_stderr() {
  { OUTPUT=$("$@" 2>&1 1>&3-); } 3>&1
}

# Prepare work directory.
rm -rf "$REPO_DIR"
mkdir -p "$REPO_DIR"

# Ensure chart dependencies are up to date.
echo "Validating the dependencies"
scripts/helm-dependencies.sh validate

# Read the shared scripting config.
source "distribution/helm/scripting-config.sh"

# Package our charts.
for CHART in "${CHARTS_TO_PUBLISH[@]}"; do
  helm package \
    "distribution/helm/$CHART" \
    --version "$CHART_VERSION" \
    --app-version "$APP_VERSION" \
    --destination "$REPO_DIR"
done

# Download previous manifest.
# If it doesn't exist - ignore the error and continue.
if ! capture_stderr aws s3 cp "$AWS_REPO_URL/index.yaml" "$PREVIOUS_MANIFEST"; then
  EXPECTED="^fatal error:"
  EXPECTED="$EXPECTED An error occurred \(404\) when calling the HeadObject operation:"
  EXPECTED="$EXPECTED Key \".*/index\.yaml\" does not exist$"
  if ! grep -Eq "$EXPECTED" <<<"$OUTPUT"; then
    echo "$OUTPUT" >&2
    exit 1
  else
    echo "Warning: repo index file doesn't exist, but we ignore the error" \
      "because we will initialize it"
  fi
fi

# Update the repo index file.
helm repo index \
  "$REPO_DIR" \
  --merge "$PREVIOUS_MANIFEST" \
  --url "$PUBLIC_URL"

# Upload new files to the repo.
aws s3 cp "$REPO_DIR" "$AWS_REPO_URL" --recursive --sse --acl public-read
