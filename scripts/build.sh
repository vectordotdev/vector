#!/usr/bin/env bash
set -euo pipefail

# build.sh
#
# SUMMARY
#
#   Used to build a binary for the specified $TARGET
#
# ENV VARS
#
#   $OVERWRITE      overwrite Vector binary even if it already exists (default "true")
#   $CHANNEL        the release channel for the build, "nightly" or "stable" (default `cargo vdev release channel`)
#   $FEATURES       a list of Vector features to include when building (default "default")
#   $NATIVE_BUILD   whether to pass the --target flag when building via cargo (default "true")
#   $TARGET         a target triple. ex: x86_64-apple-darwin (no default)

#
# Env Vars
#

OVERWRITE=${OVERWRITE:-"true"}
FEATURES="${FEATURES:-"default"}"
NATIVE_BUILD="${NATIVE_BUILD:-"true"}"
TARGET="${TARGET:?"You must specify a target triple, ex: x86_64-apple-darwin"}"

CHANNEL=${CHANNEL:-"$(cargo vdev release channel)"}
if [ "$CHANNEL" == "nightly" ]; then
  FEATURES="$FEATURES nightly"
fi

#
# Local Vars
#

if [ "$NATIVE_BUILD" != "true" ]; then
  TARGET_DIR="target/$TARGET"
else
  TARGET_DIR="target"
fi

BINARY_PATH="$TARGET_DIR/release/vector"

#
# Abort early if possible
#

if [ -f "$BINARY_PATH" ] && [ "$OVERWRITE" == "false" ]; then
  echo "Vector binary already exists at:"
  echo ""
  echo "    $BINARY_PATH"
  echo ""
  echo "Remove the binary or set ABORT to \"false\"."

  exit 0
fi

#
# CFLAGS
#

export CFLAGS="$CFLAGS -g0 -O3"

#
# Header
#

echo "Building Vector binary"
echo "OVERWRITE: $OVERWRITE"
echo "FEATURES: $FEATURES"
echo "NATIVE_BUILD: $NATIVE_BUILD"
echo "TARGET: $TARGET"
echo "Binary path: $BINARY_PATH"

#
# Build
#

BUILD_FLAGS=("--release")

if [ "$NATIVE_BUILD" != "true" ]; then
  BUILD_FLAGS+=("--target" "$TARGET")
fi

if [ "$FEATURES" == "default" ]; then
  cargo build "${BUILD_FLAGS[@]}"
else
  cargo build "${BUILD_FLAGS[@]}" --no-default-features --features "$FEATURES"
fi
