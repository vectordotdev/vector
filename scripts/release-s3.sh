#!/usr/bin/env bash

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

set -eu

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

  # Verify that the files exist and can be downloaded
  curl https://packages.timber.io/vector/nightly/$today/vector-x86_64-unknown-linux-musl.tar.gz --output "$td/today.tar.gz" --fail
  curl https://packages.timber.io/vector/nightly/latest/vector-x86_64-unknown-linux-musl.tar.gz --output "$td/latest.tar.gz" --fail
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

  # Verify that the files exist and can be downloaded
  curl https://packages.timber.io/vector/$VERSION/vector-x86_64-unknown-linux-musl.tar.gz --output "$td/$VERSION.tar.gz" --fail
  curl https://packages.timber.io/vector/latest/vector-x86_64-unknown-linux-musl.tar.gz --output "$td/latest.tar.gz" --fail
fi

#
# Cleanup
#

rm -rf $td