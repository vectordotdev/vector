#!/usr/bin/env bash

set -eu

# RPM has a concept of releases, but we do not need this so every
# release is 1.
export RELEASE=1

# The RPM spec does not like a leading `v` or `-` in the version name.
# Therefore we clean the version so that the `rpmbuild` command does
# not fail.
export CLEANED_VERSION=$VERSION
CLEANED_VERSION=$(echo $CLEANED_VERSION | sed 's/-/\./g')

# Create source dir
rm -rf /root/rpmbuild/SOURCES
mkdir -p /root/rpmbuild/SOURCES
mkdir -p /root/rpmbuild/SOURCES/init.d
mkdir -p /root/rpmbuild/SOURCES/systemd
cp -av distribution/init.d/. /root/rpmbuild/SOURCES/init.d
cp -av distribution/systemd/. /root/rpmbuild/SOURCES/systemd

for archive in target/artifacts/*.tar.gz; do
  [ -f "$archive" ] || break

  # Skip archives that are not generic linux since they are not relevant
  # to Debian. For example, we do not want to create a deb file for
  # x86_64-apple-darwin targets.
  [[ "$archive" == *"linux"* ]] || continue

  # The target is the last part of the file name.
  target=$(echo ${archive#target/artifacts/vector-$VERSION-} | sed "s|.tar.gz||g")
  echo "Target: $target"

  # The arch is the first part of the part
  arch=$(echo $target | cut -d'-' -f1)

  # Copy the
  cp -a $archive "/root/rpmbuild/SOURCES/vector-$VERSION-$arch.tar.gz"

  # Perform the build.
  # Calling rpmbuild with --target tells RPM everything it needs to know
  # about where the build can run, including the architecture.
  rpmbuild --target $target -ba distribution/rpm/vector.spec

  # Move the RPM into the artifacts dir
  mv -v "/root/rpmbuild/RPMS/$arch/vector-$CLEANED_VERSION-$RELEASE.$arch.rpm" "target/artifacts/vector-$VERSION-$arch.rpm"
done