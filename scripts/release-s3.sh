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


#
# S3
#

echo "Uploading all artifacts to s3://packages.timber.io/vector/$VERSION/"
aws s3 cp "target/artifacts/" "s3://packages.timber.io/vector/$VERSION/" --recursive

# Update the "edge" files
echo "Uploading all artifacts to s3://packages.timber.io/vector/edge/"
td=$(mktemp -d)
cp -a "target/artifacts/." "$td"
rename -v "s/$VERSION/edge/" "$td/*"
aws s3 rm --recursive "s3://packages.timber.io/vector/edge/"
aws s3 cp "$td" "s3://packages.timber.io/vector/edge/" --recursive
rm -rf $td

if [[ "$CHANNEL" == "latest" ]]; then
  # Update the "latest" files
  echo "Uploading all artifacts to s3://packages.timber.io/vector/latest/"
  td=$(mktemp -d)
  cp -a "target/artifacts/." "$td"
  rename -v "s/$VERSION/latest/" "$td/*"
  aws s3 rm --recursive "s3://packages.timber.io/vector/latest/"
  aws s3 cp "$td" "s3://packages.timber.io/vector/latest/" --recursive
  rm -rf $td
fi

#
# Packages
#

if [[ "$CHANNEL" == "latest" ]]; then
  echo "Dsitributing packages via Package Cloud"

  # Debian
  package_cloud push timberio/packages/debian/jessie target/artifacts/*.deb
  package_cloud push timberio/packages/debian/stretch target/artifacts/*.deb

  # Ubuntu
  package_cloud push timberio/packages/ubuntu/xenial target/artifacts/*.deb
  package_cloud push timberio/packages/ubuntu/zesty target/artifacts/*.deb
  package_cloud push timberio/packages/ubuntu/bionic target/artifacts/*.deb
  package_cloud push timberio/packages/ubuntu/disco target/artifacts/*.deb

  # Enterprise Linux (CentOS, RedHat, Amazon Linux)
  package_cloud push timberio/packages/el/6 target/artifacts/*.rpm
  package_cloud push timberio/packages/el/7 target/artifacts/*.rpm

  scripts/release/release_homebrew.sh
fi

#
# Github
#

if [[ "$CHANNEL" == "latest" ]]; then
  echo "Adding release to Github"
  grease create-release timberio/vector $VERSION $CIRCLE_SHA1 --assets "target/artifacts/*"
fi


#
# Install script
#

# echo "Updating sh.vector.dev install.sh script"
# aws s3api put-object \
#   --bucket "sh.vector.dev" \
#   --key "install.sh" \
#   --body "distribution/install.sh" \
#   --acl "public-read"

#
# Docker
# Install this last since the build process depends on the above.
#

if [[ "$CHANNEL" == "latest" ]]; then
  echo "Releasing timberio/vector* Docker images"
  docker build -t timberio/vector:$VERSION distribution/docker
  docker build -t timberio/vector-slim:$VERSION distribution/docker/slim
  docker build -t timberio/vector:latest distribution/docker
  docker build -t timberio/vector-slim:latest distribution/docker/slim

  docker login -u "$DOCKER_USERNAME" -p "$DOCKER_PASSWORD"
  docker push timberio/vector:$VERSION
  docker push timberio/vector-slim:$VERSION
  docker push timberio/vector:latest
  docker push timberio/vector-slim:latest
fi