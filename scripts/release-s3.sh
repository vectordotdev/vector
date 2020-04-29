#!/usr/bin/env bash

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

set -euo pipefail

CHANNEL=${CHANNEL:-$(scripts/util/release-channel.sh)}
VERSION=${VERSION:-$(scripts/version.sh)}
DATE=${DATE:-$(date -u +%Y-%m-%d)}
VERIFY_TIMEOUT=30 # seconds
VERIFY_RETRIES=2

#
# Setup
#

td=$(mktemp -d)
cp -av "target/artifacts/." "$td"
ls $td

#
# A helper function for verifying a published artifact.
#
function verify_artifact() {
  url=$1
  filename=$2
  echo "Verifying $url"
  cmp <(wget -qO- --retry-on-http-error=404 --wait 10 --tries $VERIFY_RETRIES $url) $filename
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

  # Set up redirects for historical locations
  echo "Setting up redirects for historical locations"
  aws s3api put-object \
    --bucket packages.timber.io \
    --key vector/nightly/latest/vector-x86_64-unknown-linux-gnu.tar.gz \
    --website-redirect-location /vector/nightly/latest/vector-x86_64-unknown-linux-musl.tar.gz \
    --acl public-read

  # Verify that the files exist and can be downloaded
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  sleep $VERIFY_TIMEOUT
  verify_artifact \
    https://packages.timber.io/vector/nightly/$DATE/vector-x86_64-unknown-linux-musl.tar.gz \
    $td/vector-x86_64-unknown-linux-musl.tar.gz
  verify_artifact \
    https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-musl.tar.gz \
    $td/vector-x86_64-unknown-linux-musl.tar.gz
  verify_artifact \
    https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-gnu.tar.gz \
    $td/vector-x86_64-unknown-linux-musl.tar.gz
elif [[ "$CHANNEL" == "latest" ]]; then
  version_exact=$VERSION
  version_minor_x=$(echo $VERSION | sed 's/\.[0-9]*$/.X/g')
  version_major_x=$(echo $VERSION | sed 's/\.[0-9]*\.[0-9]*$/.X/g')

  for i in $version_exact $version_minor_x $version_major_x latest; do
    # Upload the specific version
    echo "Uploading artifacts to s3://packages.timber.io/vector/$i/"
    aws s3 cp "$td" "s3://packages.timber.io/vector/$i/" --recursive --sse --acl public-read
  done
  echo "Uploaded archives"

  # Set up redirects for historical locations
  echo "Setting up redirects for historical locations"
  aws s3api put-object \
    --bucket packages.timber.io \
    --key vector/latest/vector-x86_64-unknown-linux-gnu.tar.gz \
    --website-redirect-location /vector/latest/vector-x86_64-unknown-linux-musl.tar.gz \
    --acl public-read

  # Verify that the files exist and can be downloaded
  sleep $VERIFY_TIMEOUT
  echo "Waiting for $VERIFY_TIMEOUT seconds before running the verifications"
  for i in $version_exact $version_minor_x $version_major_x latest; do
    verify_artifact \
      https://packages.timber.io/vector/$i/vector-x86_64-unknown-linux-musl.tar.gz \
      $td/vector-x86_64-unknown-linux-musl.tar.gz
  done
  verify_artifact \
    https://packages.timber.io/vector/latest/vector-x86_64-unknown-linux-gnu.tar.gz --fail \
    $td/vector-x86_64-unknown-linux-musl.tar.gz
fi

#
# Cleanup
#

rm -rf $td
