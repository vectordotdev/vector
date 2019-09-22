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
# Nightly
#

echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/"
td=$(mktemp -d)
cp -a "target/artifacts/." "$td"

# Add nightly files with today's date for posterity
rename -v "s/$escaped_version/$today/" $td/*
echo "Renamed all builds: via \"s/$escaped_version/$today/\""
ls $td
aws s3 cp "$td" "s3://packages.timber.io/vector/nightly/" --recursive
echo "Uploaded archives"

# Add nightly files with "nightly" to represent the latest nightly build
rename -v "s/$today/nightly/" $td/*
echo "Renamed all builds: via \"s/$today/nightly/\""
ls $td
aws s3 rm --recursive "s3://packages.timber.io/vector/nightly/vector-nightly"
aws s3 cp "$td" "s3://packages.timber.io/vector/nightly/" --recursive
rm -rf $td
echo "Uploaded archives"

#
# Latest
#

if [[ "$CHANNEL" == "latest" ]]; then
  # Update the "latest" files
  echo "Uploading all artifacts to s3://packages.timber.io/vector/latest/"
  td=$(mktemp -d)
  cp -a "target/artifacts/." "$td"
  rename -v "s/$escaped_version/latest/" $td/*
  echo "Renamed all builds: via \"s/$escaped_version/latest/\""
  ls $td
  aws s3 rm --recursive "s3://packages.timber.io/vector/latest/"
  aws s3 cp "$td" "s3://packages.timber.io/vector/latest/" --recursive
  rm -rf $td
  echo "Uploaded archives"
fi