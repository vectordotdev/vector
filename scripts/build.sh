#!/usr/bin/env bash

# build.sh
#
# SUMMARY
#
#   Used to build a binary for the specified $TARGET
#
# ENV VARS
#
#   $ABORT          abort if the Vector binary already exists (default "false")
#   $CHANNEL        the release channel for the build, "nighly" or "stable" (default `scripts/util/release-channel.sh`)
#   $FEATURES       a list of Vector features to include when building (default "default")
#   $NATIVE_BUILD   whether to pass the --target flag when building via cargo (default "true")
#   $STRIP          whether or not to strip the binary (default "false")
#   $TARGET         a target triple. ex: x86_64-apple-darwin (no default)

#
# Env Vars
#

ABORT=${ABORT:-false}
FEATURES=${FEATURES:-}
NATIVE_BUILD=${NATIVE_BUILD:-true}
STRIP=${STRIP:-false}
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

if [ -f "$binary_path" ] && [ "$ABORT" == "true" ]; then
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
echo "ABORT: $ABORT"
echo "FEATURES: $FEATURES"
echo "NATIVE_BUILD: $NATIVE_BUILD"
echo "STRIP: $STRIP"
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

if [ "$STRIP" == "true" ]; then
  strip $binary_path
fi
