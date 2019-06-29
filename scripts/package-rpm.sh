#!/usr/bin/env bash

# package-rpm.sh
#
# SUMMARY
#
#   Packages a .rpm file to be distributed in the YUM package manager.

set -eu

archive_name="vector-$VERSION-$TARGET.tar.gz"
archive_path="target/artifacts/$archive_name"

# RPM has a concept of releases, but we do not need this so every
# release is 1.
export RELEASE=1

# The RPM spec does not like a leading `v` or `-` in the version name.
# Therefore we clean the version so that the `rpmbuild` command does
# not fail.
export CLEANED_VERSION=$VERSION
CLEANED_VERSION=$(echo $CLEANED_VERSION | sed 's/-/\./g')

# The arch is the first part of the part
ARCH=$(echo $TARGET | cut -d'-' -f1)

# Create source dir
rm -rf /root/rpmbuild/SOURCES
mkdir -p /root/rpmbuild/SOURCES
mkdir -p /root/rpmbuild/SOURCES/init.d
mkdir -p /root/rpmbuild/SOURCES/systemd
cp -av distribution/init.d/. /root/rpmbuild/SOURCES/init.d
cp -av distribution/systemd/. /root/rpmbuild/SOURCES/systemd

# Copy the archive into the sources dir
cp -a $archive_path "/root/rpmbuild/SOURCES/vector-$VERSION-$ARCH.tar.gz"

# Perform the build.
# Calling rpmbuild with --target tells RPM everything it needs to know
# about where the build can run, including the architecture.
rpmbuild --target $TARGET -ba distribution/rpm/vector.spec

# Move the RPM into the artifacts dir
mv -v "/root/rpmbuild/RPMS/$ARCH/vector-$CLEANED_VERSION-$RELEASE.$ARCH.rpm" "target/artifacts/vector-$VERSION-$ARCH.rpm"