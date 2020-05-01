#!/usr/bin/env bash

# package-rpm.sh
#
# SUMMARY
#
#   Packages a .rpm file to be distributed in the YUM package manager.
#
# ENV VARS
#
#   $TARGET         a target triple. ex: x86_64-apple-darwin (no default)

#
# Local vars
#

project_root=$(pwd)
archive_name="vector-$TARGET.tar.gz"
archive_path="target/artifacts/$archive_name"
package_version="$($project_root/scripts/version.sh)"

#
# Header
#

set -eu

echo "Packaging .rpm for $archive_name"
echo "TARGET: $TARGET"

#
# Safeguard
#

if [[ "$UID" == "0" ]]; then
  echo "Error: aborting RPM build due to execution as root" >&2
  exit 1
fi

#
# Package
#

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
rm -rf "$HOME/rpmbuild/SOURCES"
mkdir -p "$HOME/rpmbuild/SOURCES"
mkdir -p "$HOME/rpmbuild/SOURCES/init.d"
mkdir -p "$HOME/rpmbuild/SOURCES/systemd"
cp -av distribution/init.d/. "$HOME/rpmbuild/SOURCES/init.d"
cp -av distribution/systemd/. "$HOME/rpmbuild/SOURCES/systemd"

# Copy the archive into the sources dir
cp -av $archive_path "$HOME/rpmbuild/SOURCES/vector-$ARCH.tar.gz"

# Perform the build.
rpmbuild \
  --define "_topdir $HOME/rpmbuild" \
  --target "$ARCH-redhat-linux" \
  --define "_arch $ARCH" \
  -ba distribution/rpm/vector.spec

#
# Move the RPM into the artifacts dir
#

ls "$HOME/rpmbuild/RPMS/$ARCH"
mv -v "$HOME/rpmbuild/RPMS/$ARCH/vector-$CLEANED_VERSION-$RELEASE.$ARCH.rpm" "target/artifacts/vector-$ARCH.rpm"
