#!/usr/bin/env bash

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

set -euo pipefail

CHANNEL=$(scripts/util/release-channel.sh)

#
# Setup
#

td=$(mktemp -d)
cp -av "target/artifacts/." "$td"
ls $td

#
# Upload
#

if [[ "$CHANNEL" == "nightly" ]]; then
  # Add nightly files with today's date for posterity
  today=$(date +"%F")
  echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/$today"
  aws s3 cp "$td" "s3://packages.timber.io/vector/nightly/$today" --recursive --sse --acl public-read 
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
  cmp <(curl https://packages.timber.io/vector/nightly/$today/vector-x86_64-unknown-linux-musl.tar.gz --fail) "$td/vector-x86_64-unknown-linux-musl.tar.gz"
  cmp <(curl https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-musl.tar.gz --fail) "$td/vector-x86_64-unknown-linux-musl.tar.gz"
  cmp <(curl -L https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-gnu.tar.gz --fail) "$td/vector-x86_64-unknown-linux-musl.tar.gz"
elif [[ "$CHANNEL" == "latest" ]]; then
  # Upload the specific version
  echo "Uploading all artifacts to s3://packages.timber.io/vector/$VERSION/"
  aws s3 cp "$td" "s3://packages.timber.io/vector/$VERSION/" --recursive --sse --acl public-read
  echo "Uploaded archives"

  # Update the "latest" files
  echo "Uploading all artifacts to s3://packages.timber.io/vector/latest/"
  aws s3 rm --recursive "s3://packages.timber.io/vector/latest/"
  aws s3 cp "$td" "s3://packages.timber.io/vector/latest/" --recursive --sse --acl public-read
  echo "Uploaded archives"

  # Set up redirects for historical locations
  echo "Setting up redirects for historical locations"
  aws s3api put-object \
    --bucket packages.timber.io \
    --key vector/latest/vector-x86_64-unknown-linux-gnu.tar.gz \
    --website-redirect-location /vector/latest/vector-x86_64-unknown-linux-musl.tar.gz \
    --acl public-read

  # Verify that the files exist and can be downloaded
  cmp <(curl https://packages.timber.io/vector/$VERSION/vector-x86_64-unknown-linux-musl.tar.gz --fail) "$td/vector-x86_64-unknown-linux-musl.tar.gz"
  cmp <(curl https://packages.timber.io/vector/latest/vector-x86_64-unknown-linux-musl.tar.gz --fail) "$td/vector-x86_64-unknown-linux-musl.tar.gz"
  cmp <(curl -L https://packages.timber.io/vector/latest/vector-x86_64-unknown-linux-gnu.tar.gz --fail) "$td/vector-x86_64-unknown-linux-musl.tar.gz"
fi

#
# Cleanup
#

rm -rf $td