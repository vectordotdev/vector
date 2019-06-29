#!/usr/bin/env bash

# release.sh
#
# SUMMARY
#
#   Releases Vector to S3, APT, YUM, Homebrew, Docker, and Github

set -eu

#
# S3
#

aws s3 cp "target/artifacts/" "s3://packages.timber.io/vector/$VERSION/" --recursive

# Update the "latest" files
mkdir -p target/latest
cp -a target/artifacts/. target/latest
rename -v "s/$VERSION/latest/" target/latest/*
aws s3 rm --recursive s3://packages.timber.io/vector/latest/
aws s3 cp "target/latest/" "s3://packages.timber.io/vector/latest/" --recursive
rm -rf target/latest

#
# Packages
#

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

#
# Docker
#

docker build -t timberio/vector:$VERSION distribution/docker
docker build -t timberio/vector-slim:$VERSION distribution/docker/slim
docker build -t timberio/vector:latest distribution/docker
docker build -t timberio/vector-slim:latest distribution/docker/slim

docker login -u "$DOCKER_USERNAME" -p "$DOCKER_PASSWORD"
docker push timberio/vector:$VERSION
docker push timberio/vector-slim:$VERSION
docker push timberio/vector:latest
docker push timberio/vector-slim:latest

#
# Github
#

grease create-release timberio/vector $VERSION $CIRCLE_SHA1 --assets "target/artifacts/*"

#
# Homebrew
#

# scripts/release/release_homebrew.sh