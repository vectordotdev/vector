#!/usr/bin/env bash

# build.sh
#
# SUMMARY
#
#   Used to build a tar.gz archive for the specified $TARGET and $VERSION
#
# ENV VARS
#
#   $FEATURES - a list of Vector features to include when building, defaults to all
#   $NATIVE_BUILD - whether to pass the --target flag when building via cargo
#   $TARGET - a target triple. ex: x86_64-apple-darwin
#   $VERSION - the version of Vector, can be obtained via `make version`

NATIVE_BUILD=${NATIVE_BUILD:-}

set -eu

echo "Building Vector archive"
echo "Version: $VERSION"
echo "Target: $TARGET"
echo "Native build: $NATIVE_BUILD"
echo "Features: $FEATURES"

# Setup directories
artifacts_dir="target/artifacts"

if [ -z "$NATIVE_BUILD" ]; then
  target_dir="target/$TARGET"
else
  target_dir="target"
fi

archive_dir_name="vector-$VERSION"
archive_dir="$target_dir/$archive_dir_name"

# Build
build_flags="--release"

if [ -z "$NATIVE_BUILD" ]; then
  build_flags="$build_flags --target $TARGET"
fi

if [ "$FEATURES" != "default" ]; then
  build_flags="$build_flags --no-default-features --features $FEATURES"
fi

cargo build $build_flags

# Build the archive directory
rm -rf $archive_dir
mkdir -p $archive_dir

# Copy root level files
cp -a README.md $archive_dir
cp -a LICENSE $archive_dir

# Copy the vector binary to /bin
mkdir -p $archive_dir/bin
cp -a $target_dir/release/vector $archive_dir/bin

# Copy the entire config dir to /config
cp -rv config $archive_dir/config

# Copy /etc usefule files
mkdir -p $archive_dir/etc/systemd
cp -a distribution/systemd/vector.service $archive_dir/etc/systemd
mkdir -p $archive_dir/etc/init.d
cp -a distribution/init.d/vector $archive_dir/etc/init.d

# Build the release tar
_old_dir=$(pwd)
cd $target_dir
tar -czvf vector-$VERSION-$TARGET.tar.gz ./$archive_dir_name
cd $_old_dir

# Move to the artifacts dir
mkdir -p $artifacts_dir
mv -v $target_dir/vector-$VERSION-$TARGET.tar.gz $artifacts_dir
echo "Moved $target_dir/vector-$VERSION-$TARGET.tar.gz to $artifacts_dir"

# Cleanup
rm -rf $archive_dir