#!/usr/bin/env bash

# build.sh
#
# SUMMARY
#
#   Used to build a binary for the specified $TARGET
#
# ENV VARS
#
#   $OVERWRITE      overwrite Vector binary even if it already exists (default "true")
#   $CHANNEL        the release channel for the build, "nighly" or "stable" (default `scripts/util/release-channel.sh`)
#   $FEATURES       a list of Vector features to include when building (default "default")
#   $NATIVE_BUILD   whether to pass the --target flag when building via cargo (default "true")
#   $KEEP_SYMBOLS   whether to keep the any debug symbols in the binaries or not (default "true")
#   $TARGET         a target triple. ex: x86_64-apple-darwin (no default)

#
# Env Vars
#

OVERWRITE=${OVERWRITE:-true}
FEATURES=${FEATURES:-}
NATIVE_BUILD=${NATIVE_BUILD:-true}
KEEP_SYMBOLS=${KEEP_SYMBOLS:-true}
TARGET=${TARGET:-}

if [ -z "$FEATURES" ]; then
  FEATURES="default"
fi

CHANNEL=${CHANNEL:-$(scripts/util/release-channel.sh)}
if [ "$CHANNEL" == "nightly" ]; then
  FEATURES="$FEATURES nightly"
fi

#
# Local Vars
#

if [ "$NATIVE_BUILD" != "true" ]; then
  target_dir="target/$TARGET"
else
  target_dir="target"
fi

binary_path="$target_dir/release/vector"

#
# Abort early if possible
#

if [ -f "$binary_path" ] && [ "$OVERWRITE" == "false" ]; then
  echo "Vector binary already exists at:"
  echo ""
  echo "    $binary_path"
  echo ""
  echo "Remove the binary or set ABORT to \"false\"."

  exit 0
fi

#
# Header
#

set -eu

echo "Building Vector binary"
echo "OVERWRITE: $OVERWRITE"
echo "FEATURES: $FEATURES"
echo "NATIVE_BUILD: $NATIVE_BUILD"
echo "KEEP_SYMBOLS: $KEEP_SYMBOLS"
echo "TARGET: $TARGET"
echo "Binary path: $binary_path"

#
# Build
#

build_flags="--release"

if [ "$NATIVE_BUILD" != "true" ]; then
  build_flags="$build_flags --target $TARGET"
fi

on_exit=""

if [ "$FEATURES" != "default" ]; then
  cargo build $build_flags --no-default-features --features "$FEATURES"
else
  cargo build $build_flags
fi

#
# Strip the output binary
#

if [ "$KEEP_SYMBOLS" == "false" ]; then
  strip $binary_path
fi
