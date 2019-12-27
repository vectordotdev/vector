#!/usr/bin/env bash

# package-rpm.sh
#
# SUMMARY
#
#   Packages a .rpm file to be distributed in the YUM package manager.

set -eu

project_root=$(pwd)
archive_name="vector-$TARGET.tar.gz"
archive_path="target/artifacts/$archive_name"
package_version="$($project_root/scripts/version.sh)"

# RPM has a concept of releases, but we do not need this so every
# release is 1.
export RELEASE=1

# The RPM spec does not like a leading `v` or `-` in the version name.
# Therefore we clean the version so that the `rpmbuild` command does
# not fail.
export CLEANED_VERSION=$package_version
CLEANED_VERSION=$(echo $CLEANED_VERSION | sed 's/-/\./g')

# The arch is the first part of the target
# For some architectures, like armv7hl it doesn't match the arch
# from Rust target triple and needs to be specified manually.
ARCH=${ARCH:-$(echo $TARGET | cut -d'-' -f1)}

# Create source dir
rm -rf /root/rpmbuild/SOURCES
mkdir -p /root/rpmbuild/SOURCES
mkdir -p /root/rpmbuild/SOURCES/init.d
mkdir -p /root/rpmbuild/SOURCES/systemd
cp -av distribution/init.d/. /root/rpmbuild/SOURCES/init.d
cp -av distribution/systemd/. /root/rpmbuild/SOURCES/systemd

# Copy the archive into the sources dir
cp -av $archive_path "/root/rpmbuild/SOURCES/vector-$ARCH.tar.gz"

# Perform the build.
rpmbuild --target "$ARCH-redhat-linux" --define "_arch $ARCH" -ba distribution/rpm/vector.spec

# Move the RPM into the artifacts dir
ls "/root/rpmbuild/RPMS/$ARCH"
mv -v "/root/rpmbuild/RPMS/$ARCH/vector-$CLEANED_VERSION-$RELEASE.$ARCH.rpm" "target/artifacts/vector-$ARCH.rpm"
