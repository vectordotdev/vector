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

# Prepare rpmbuild dir
RPMBUILD_DIR="$(mktemp -td "rpmbuild.XXXX")"

# Create build dirs
for ITEM in RPMS SOURCES SPECS SRPMS BUILD; do
  rm -rf "${RPMBUILD_DIR:?}/${ITEM:?}"
  mkdir -p "$RPMBUILD_DIR/$ITEM"
done

# Init support data
mkdir -p \
  "$RPMBUILD_DIR/SOURCES/init.d" \
  "$RPMBUILD_DIR/SOURCES/systemd"
cp -av distribution/init.d/. "$RPMBUILD_DIR/SOURCES/init.d"
cp -av distribution/systemd/. "$RPMBUILD_DIR/SOURCES/systemd"

# Copy the archive into the sources dir
cp -av $archive_path "$RPMBUILD_DIR/SOURCES/vector-$ARCH.tar.gz"

# Perform the build.
rpmbuild \
  --define "_topdir $RPMBUILD_DIR" \
  --target "$ARCH-redhat-linux" \
  --define "_arch $ARCH" \
  -ba distribution/rpm/vector.spec

#
# Move the RPM into the artifacts dir
#

ls "$RPMBUILD_DIR/RPMS/$ARCH"
mv -v "$RPMBUILD_DIR/RPMS/$ARCH/vector-$CLEANED_VERSION-$RELEASE.$ARCH.rpm" "target/artifacts/vector-$ARCH.rpm"
