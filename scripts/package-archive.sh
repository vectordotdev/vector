#!/usr/bin/env bash
set -euo pipefail

# package-archive.sh
#
# SUMMARY
#
#   Used to package a tar.gz archive for the specified $TARGET. This assumes
#   that the binary is already built and in the proper $TARGET dir. See
#   build.sh for more info.
#
# ENV VARS
#
#   $OVERWRITE      overwrite Vector binary even if it already exists (default "true")
#   $ARCHIVE_TYPE   archive type, either "tar.gz" or "zip" (default "tar.gz")
#   $NATIVE_BUILD   whether the binary was built natively or with a --target (default "true")
#   $TARGET         a target triple. ex: x86_64-apple-darwin (no default)

#
# Env Vars
#

OVERWRITE=${OVERWRITE:-"true"}
ARCHIVE_TYPE="${ARCHIVE_TYPE:-"tar.gz"}"
NATIVE_BUILD="${NATIVE_BUILD:-"true"}"
TARGET="${TARGET:?"You must specify a target triple, ex: x86_64-apple-darwin"}"
ARCHIVE_VERSION="${VECTOR_VERSION:-"$(cargo vdev version)"}"

#
# Local Vars
#

if [ "$NATIVE_BUILD" != "true" ]; then
  TARGET_DIR="target/$TARGET"
else
  TARGET_DIR="target"
fi

ARCHIVE_DIR_NAME="vector-$TARGET"
ARCHIVE_DIR="$TARGET_DIR/$ARCHIVE_DIR_NAME"
ARCHIVE_NAME="vector-$ARCHIVE_VERSION-$TARGET.$ARCHIVE_TYPE"
ARTIFACTS_DIR="target/artifacts"

#
# Abort if possible
#

if [ -f "$ARTIFACTS_DIR/$ARCHIVE_NAME" ] && [ "$OVERWRITE" == "false" ]; then
  echo "Archive already exists at:"
  echo ""
  echo "    $ARTIFACTS_DIR/$ARCHIVE_NAME"
  echo ""
  echo "Remove the archive or set ABORT to \"false\"."

  exit 0
fi

#
# Header
#

echo "Packaging the Vector archive"
echo "OVERWRITE: $OVERWRITE"
echo "ARCHIVE_TYPE: $ARCHIVE_TYPE"
echo "NATIVE_BUILD: $NATIVE_BUILD"
echo "TARGET: $TARGET"

#
# Build the archive directory
#

rm -rf "$ARCHIVE_DIR"
mkdir -p "$ARCHIVE_DIR"

#
# Copy files to archive dir
#

# Copy root level files

if [[ $TARGET == *windows* ]]; then
  SUFFIX=".txt"
else
  SUFFIX=""
fi
cp -av README.md "$ARCHIVE_DIR/README.md$SUFFIX"
# Create the license file for binary distributions (LICENSE + NOTICE)
cat LICENSE NOTICE > "$ARCHIVE_DIR/LICENSE$SUFFIX"

cp -av licenses "$ARCHIVE_DIR/licenses"
cp -av LICENSE-3rdparty.csv "$ARCHIVE_DIR"

# Copy the vector binary to /bin

mkdir -p "$ARCHIVE_DIR/bin"
cp -av "$TARGET_DIR/release/vector" "$ARCHIVE_DIR/bin"

# Copy the entire config dir to /config

cp -rv config "$ARCHIVE_DIR/config"

# Copy /etc useful files

if [[ $TARGET == *linux* ]]; then
  mkdir -p "$ARCHIVE_DIR/etc/systemd"
  cp -av distribution/systemd/vector.service "$ARCHIVE_DIR/etc/systemd"
  mkdir -p "$ARCHIVE_DIR/etc/init.d"
  cp -av distribution/init.d/vector "$ARCHIVE_DIR/etc/init.d"
fi

#
# Build the release archive
#

(
  cd "$TARGET_DIR"
  if [ "$ARCHIVE_TYPE" == "tar.gz" ]; then
    tar cvf - "./$ARCHIVE_DIR_NAME" | gzip -9 > "$ARCHIVE_NAME"
  elif [ "$ARCHIVE_TYPE" == "zip" ] && [[ $TARGET == *windows* ]]; then
    # shellcheck disable=SC2016
    powershell '$progressPreference = "silentlyContinue"; Compress-Archive -DestinationPath vector-'"$ARCHIVE_VERSION"'-'"$TARGET"'.'"$ARCHIVE_TYPE"' -Path "./'"$ARCHIVE_DIR_NAME"'/*"'
  else
    echo "Unsupported combination of ARCHIVE_TYPE and TARGET"
    exit 1
  fi
)

#
# Move to the artifacts dir
#

mkdir -p "$ARTIFACTS_DIR"
mv -v "$TARGET_DIR/$ARCHIVE_NAME" "$ARTIFACTS_DIR"
echo "Moved $TARGET_DIR/$ARCHIVE_NAME to $ARTIFACTS_DIR"

#
# Cleanup
#

rm -rf "$ARCHIVE_DIR"
