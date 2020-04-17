#!/usr/bin/env bash

# build.sh
#
# SUMMARY
#
#   Used to build a tar.gz archive for the specified $TARGET
#
# ENV VARS
#
#   $FEATURES - a list of Vector features to include when building, defaults to all
#   $NATIVE_BUILD - whether to pass the --target flag when building via cargo
#   $STRIP - whether or not to strip the binary
#   $TARGET - a target triple. ex: x86_64-apple-darwin
#   $ARCHIVE_TYPE - archive type, either "tar.gz" or "zip"

NATIVE_BUILD=${NATIVE_BUILD:-}
STRIP=${STRIP:-}
FEATURES=${FEATURES:-}
ARCHIVE_TYPE=${ARCHIVE_TYPE:-tar.gz}

if [ -z "$FEATURES" ]; then
    FEATURES="default"
fi

CHANNEL="$(scripts/util/release-channel.sh)"
if [ "$CHANNEL" == "nightly" ]; then
  FEATURES="$FEATURES nightly"
fi

set -eu

echo "Building Vector archive"
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

archive_dir_name="vector-$TARGET"
archive_dir="$target_dir/$archive_dir_name"

# Build
build_flags="--release"

if [ -z "$NATIVE_BUILD" ]; then
  build_flags="$build_flags --target $TARGET"
fi

on_exit=""

if [ "$FEATURES" != "default" ]; then
    cargo build $build_flags --no-default-features --features "$FEATURES"
else
    cargo build $build_flags
fi


# Strip the output binary
if [ "$STRIP" != "false" ]; then
  strip $target_dir/release/vector
fi

# Build the archive directory
rm -rf $archive_dir
mkdir -p $archive_dir

# Copy root level files

if [[ $TARGET == *windows* ]]; then
  suffix=".txt"
else
  suffix=""
fi
cp -av README.md $archive_dir/README.md$suffix
# Create the license file for binary distributions (LICENSE + NOTICE)
cat LICENSE NOTICE > $archive_dir/LICENSE$suffix

# Copy the vector binary to /bin
mkdir -p $archive_dir/bin
cp -av $target_dir/release/vector $archive_dir/bin

# Copy the entire config dir to /config
cp -rv config $archive_dir/config
# Remove templates sources
rm $archive_dir/config/*.erb

if [[ $TARGET == *linux* ]]; then
  # Copy /etc useful files
  mkdir -p $archive_dir/etc/systemd
  cp -av distribution/systemd/vector.service $archive_dir/etc/systemd
  mkdir -p $archive_dir/etc/init.d
  cp -av distribution/init.d/vector $archive_dir/etc/init.d
fi

# Build the release archive
_old_dir=$(pwd)
cd $target_dir
if [ "$ARCHIVE_TYPE" == "tar.gz" ]; then
  tar cvf - ./$archive_dir_name | gzip -9 > vector-$TARGET.$ARCHIVE_TYPE
elif [ "$ARCHIVE_TYPE" == "zip" ] && [[ $TARGET == *windows* ]]; then
  powershell '$progressPreference = "silentlyContinue"; Compress-Archive -DestinationPath vector-'$TARGET'.'$ARCHIVE_TYPE' -Path "./'$archive_dir_name'/*"'
else
  echo "Unsupported combination of ARCHIVE_TYPE and TARGET"
  exit 1
fi
cd $_old_dir

# Move to the artifacts dir
mkdir -p $artifacts_dir
mv -v $target_dir/vector-$TARGET.$ARCHIVE_TYPE $artifacts_dir
echo "Moved $target_dir/vector-$TARGET.$ARCHIVE_TYPE to $artifacts_dir"

# Cleanup
rm -rf $archive_dir
