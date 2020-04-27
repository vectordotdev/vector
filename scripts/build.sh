#!/usr/bin/env bash

# build.sh
#
# SUMMARY
#
#   Used to build a binary for the specified $TARGET
#
# ENV VARS
#
#   $FEATURES - a list of Vector features to include when building, defaults to all
#   $NATIVE_BUILD - whether to pass the --target flag when building via cargo
#   $STRIP - whether or not to strip the binary
#   $TARGET - a target triple. ex: x86_64-apple-darwin

#
# Env vars
#

NATIVE_BUILD=${NATIVE_BUILD:-}
STRIP=${STRIP:-}
FEATURES=${FEATURES:-}

if [ -z "$FEATURES" ]; then
    FEATURES="default"
fi

CHANNEL=${CHANNEL:-$(scripts/util/release-channel.sh)}
if [ "$CHANNEL" == "nightly" ]; then
  FEATURES="$FEATURES nightly"
fi

#
# Header
#

set -eu

echo "Building Vector binary"
echo "Target: $TARGET"
echo "Native build: $NATIVE_BUILD"
echo "Features: $FEATURES"

#
# Setup directories
#

if [ -z "$NATIVE_BUILD" ]; then
  target_dir="target/$TARGET"
else
  target_dir="target"
fi

#
# Build
#

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

#
# Strip the output binary
#

if [ "$STRIP" != "false" ]; then
  strip $target_dir/release/vector
fi
