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
#   $ABORT          abort if the archive already exists (default "false")
#   $ARCHIVE_TYPE   archive type, either "tar.gz" or "zip" (default "tar.gz")
#   $NATIVE_BUILD   whether the binary was built natively or with a --target (default "true")
#   $TARGET         a target triple. ex: x86_64-apple-darwin (no default)

#
# Env Vars
#

ABORT=${ABORT:-false}
ARCHIVE_TYPE=${ARCHIVE_TYPE:-tar.gz}
NATIVE_BUILD=${NATIVE_BUILD:-true}

#
# Local Vars
#

if [ "$NATIVE_BUILD" != "true" ]; then
  target_dir="target/$TARGET"
else
  target_dir="target"
fi

archive_dir_name="vector-$TARGET"
archive_dir="$target_dir/$archive_dir_name"
archive_name="vector-$TARGET.$ARCHIVE_TYPE"
artifacts_dir="target/artifacts"

#
# Abort if possible
#

if [ -f "$artifacts_dir/$archive_name" ] && [ "$ABORT" == "true" ]; then
  echo "Archive already exists at:"
  echo ""
  echo "    $artifacts_dir/$archive_name"
  echo ""
  echo "Remove the archive or set ABORT to \"false\"."

  exit 0
fi

#
# Header
#

set -eu

echo "Packaging the Vector archive"
echo "ABORT: $ABORT"
echo "ARCHIVE_TYPE: $ARCHIVE_TYPE"
echo "NATIVE_BUILD: $NATIVE_BUILD"
echo "TARGET: $TARGET"

#
# Build the archive directory
#

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
  tar cvf - ./$archive_dir_name | gzip -9 > $archive_name
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

mkdir -p $artifacts_dir
mv -v $target_dir/$archive_name $artifacts_dir
echo "Moved $target_dir/$archive_name to $artifacts_dir"

#
# Cleanup
#

rm -rf $archive_dir
