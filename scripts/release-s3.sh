#!/usr/bin/env bash

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

set -eu

CHANNEL=$(scripts/util/release-channel.sh)
escaped_version=$(echo $VERSION | sed "s/\./\\\./g")

#
# S3
#

echo "Uploading all artifacts to s3://packages.timber.io/vector/$VERSION/"
aws s3 cp "target/artifacts/" "s3://packages.timber.io/vector/$VERSION/" --recursive

# Update the "nightly" files
echo "Uploading all artifacts to s3://packages.timber.io/vector/nightly/"
td=$(mktemp -d)

cp -a "target/artifacts/." "$td"
rename -v "s/$escaped_version/nightly/" $td/*
echo "Renamed all builds: via \"s/$escaped_version/nightly/\""
ls $td
aws s3 rm --recursive "s3://packages.timber.io/vector/nightly/"
aws s3 cp "$td" "s3://packages.timber.io/vector/nightly/" --recursive
rm -rf $td
echo "Uploaded nightly archives"

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
  echo "Uploaded latest archives"
fi