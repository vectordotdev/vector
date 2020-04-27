#!/usr/bin/env bash

# package-archive.sh
#
# SUMMARY
#
#   Used to package a tar.gz archive for the specified $TARGET. This assumes
#   that the binary is already built and in the proper $TARGET dir. See
#   buid.sh for more info.
#
# ENV VARS
#
#   $ARCHIVE_TYPE - archive type, either "tar.gz" or "zip"
#   $NATIVE_BUILD - whether to pass the --target flag when building via cargo
#   $TARGET - a target triple. ex: x86_64-apple-darwin

ARCHIVE_TYPE=${ARCHIVE_TYPE:-tar.gz}
NATIVE_BUILD=${NATIVE_BUILD:-}

set -eu

echo "Packaging the Vector archive"
echo "Archive Type: $ARCHIVE_TYPE"
echo "Target: $TARGET"

#
# Build the archive directory
#

if [ -z "$NATIVE_BUILD" ]; then
  target_dir="target/$TARGET"
else
  target_dir="target"
fi

archive_dir_name="vector-$TARGET"
archive_dir="$target_dir/$archive_dir_name"
rm -rf $archive_dir
mkdir -p $archive_dir

#
# Copy files to archive dir
#

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

# Copy /etc useful files

if [[ $TARGET == *linux* ]]; then
  mkdir -p $archive_dir/etc/systemd
  cp -av distribution/systemd/vector.service $archive_dir/etc/systemd
  mkdir -p $archive_dir/etc/init.d
  cp -av distribution/init.d/vector $archive_dir/etc/init.d
fi

#
# Build the release archive
#

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

#
# Move to the artifacts dir
#

artifacts_dir="target/artifacts"
mkdir -p $artifacts_dir
mv -v $target_dir/vector-$TARGET.$ARCHIVE_TYPE $artifacts_dir
echo "Moved $target_dir/vector-$TARGET.$ARCHIVE_TYPE to $artifacts_dir"

#
# Cleanup
#

rm -rf $archive_dir
