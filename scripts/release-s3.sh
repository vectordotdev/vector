#!/usr/bin/env bash
set -euo pipefail

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

CHANNEL="${CHANNEL:-"$(scripts/util/release-channel.sh)"}"
VERSION="${VERSION:-"$(scripts/version.sh)"}"
DATE="${DATE:-"$(date -u +%Y-%m-%d)"}"
VERIFY_TIMEOUT="${VERIFY_TIMEOUT:-"30"}" # seconds
VERIFY_RETRIES="${VERIFY_RETRIES:-"2"}"

#
# Setup
#

td="$(mktemp -d)"
cp -av "target/artifacts/." "$td"
ls "$td"

#
# A helper function for verifying a published artifact.
#
verify_artifact() {
  local URL="$1"
  local FILENAME="$2"
  echo "Verifying $URL"
  cmp <(wget -qO- --retry-on-http-error=404 --wait 10 --tries "$VERIFY_RETRIES" "$URL") "$FILENAME"
}

#
# Upload
#

if [[ "$CHANNEL" == "nightly" ]]; then
  # Add nightly files with the $DATE for posterity
  echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/$DATE"
  aws s3 cp "$td" "s3://packages.timber.io/vector/nightly/$DATE" --recursive --sse --acl public-read
  echo "Uploaded archives"

  # Add "latest" nightly files
  echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/latest"
  aws s3 rm --recursive "s3://packages.timber.io/vector/nightly/latest"
  aws s3 cp "$td" "s3://packages.timber.io/vector/nightly/latest" --recursive --sse --acl public-read
  echo "Uploaded archives"

  # Verify that the files exist and can be downloaded
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  sleep "$VERIFY_TIMEOUT"
  verify_artifact \
    "https://packages.timber.io/vector/nightly/$DATE/vector-x86_64-unknown-linux-musl.tar.gz" \
    "$td/vector-x86_64-unknown-linux-musl.tar.gz"
  verify_artifact \
    "https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-musl.tar.gz" \
    "$td/vector-x86_64-unknown-linux-musl.tar.gz"
  verify_artifact \
    "https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-gnu.tar.gz" \
    "$td/vector-x86_64-unknown-linux-musl.tar.gz"
elif [[ "$CHANNEL" == "latest" ]]; then
  VERSION_EXACT="$VERSION"
  # shellcheck disable=SC2001
  VERSION_MINOR_X="$(echo "$VERSION" | sed 's/\.[0-9]*$/.X/g')"
  # shellcheck disable=SC2001
  VERSION_MAJOR_X="$(echo "$VERSION" | sed 's/\.[0-9]*\.[0-9]*$/.X/g')"

  for i in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X" latest; do
    # Upload the specific version
    echo "Uploading artifacts to s3://packages.timber.io/vector/$i/"
    aws s3 cp "$td" "s3://packages.timber.io/vector/$i/" --recursive --sse --acl public-read
  done
  echo "Uploaded archives"

  # Verify that the files exist and can be downloaded
  sleep "$VERIFY_TIMEOUT"
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  for i in "$VERSION_EXACT" "$VERSION_MINOR_X" "$VERSION_MAJOR_X" latest; do
    verify_artifact \
      "https://packages.timber.io/vector/$i/vector-x86_64-unknown-linux-musl.tar.gz" \
      "$td/vector-x86_64-unknown-linux-musl.tar.gz"
  done
  verify_artifact \
    "https://packages.timber.io/vector/latest/vector-x86_64-unknown-linux-gnu.tar.gz" --fail \
    "$td/vector-x86_64-unknown-linux-gnu.tar.gz"
fi

#
# Cleanup
#

rm -rf "$td"
