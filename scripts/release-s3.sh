#!/usr/bin/env bash

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

set -eu

CHANNEL=$(scripts/util/release-channel.sh)
escaped_version=$(echo $VERSION | sed "s/\./\\\./g")
today=$(date +"%F")

#
# Setup
#

td=$(mktemp -d)
cp -a "target/artifacts/." "$td"
ls $td

#
# Nightly
#

if [[ "$CHANNEL" == "nightly" ]]; then
  # Add nightly files with today's date for posterity
  echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/$today"
  aws s3 cp "$td" "s3://packages.timber.io/vector/nightly/$today" --recursive
  echo "Uploaded archives"

  # Add "latest" nightly files
  echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/latest"
  aws s3 rm --recursive "s3://packages.timber.io/vector/nightly/latest"
  aws s3 cp "$td" "s3://packages.timber.io/vector/nightly/latest" --recursive
  echo "Uploaded archives"
fi

#
# Latest
#

if [[ "$CHANNEL" == "latest" ]]; then
  # Upload the specific version
  echo "Uploading all artifacts to s3://packages.timber.io/vector/$VERSION/"
  aws s3 cp "$td" "s3://packages.timber.io/vector/$VERSION/" --recursive
  echo "Uploaded archives"

  # Update the "latest" files
  echo "Uploading all artifacts to s3://packages.timber.io/vector/latest/"
  aws s3 rm --recursive "s3://packages.timber.io/vector/latest/"
  aws s3 cp "$td" "s3://packages.timber.io/vector/latest/" --recursive
  echo "Uploaded archives"
fi

#
# Cleanup
#

rm -rf $td