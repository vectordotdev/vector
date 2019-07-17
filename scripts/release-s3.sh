#!/usr/bin/env bash

# release-s3.sh
#
# SUMMARY
#
#   Uploads archives and packages to S3

set -eu

CHANNEL=""
if [[ $VERSION == *"-"* ]]; then
  CHANNEL="edge"
  echo "Version ($VERSION) is an edge version, only releasing to edge channels"
else
  CHANNEL="latest"
  echo "Version ($VERSION) is a release version, releasing to latest channels"
fi

if [ -z "$CHANNEL" ]; then
  echo 'The CHANNEL env var must be set to "edge" or "latest"'
  exit 1
fi

escaped_version=$(echo $VERSION | sed "s/\./\\\./g")

#
# S3
#

echo "Uploading all artifacts to s3://packages.timber.io/vector/$VERSION/"
aws s3 cp "target/artifacts/" "s3://packages.timber.io/vector/$VERSION/" --recursive

# Update the "edge" files
echo "Uploading all artifacts to s3://packages.timber.io/vector/edge/"
td=$(mktemp -d)
cp -a "target/artifacts/." "$td"
rename -v "s/$escaped_version/edge/" $td/*
echo "Renamed all builds: via \"s/$escaped_version/edge/\""
ls $td
aws s3 rm --recursive "s3://packages.timber.io/vector/edge/"
aws s3 cp "$td" "s3://packages.timber.io/vector/edge/" --recursive
rm -rf $td
echo "Uploaded edge archives"

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