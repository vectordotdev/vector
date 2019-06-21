#!/usr/bin/env bash

# Usage
#
# Release with cross via the default system target
# ./build_release.sh
#
# Release with the specified target
# TARGET="<my target>" ./build_release.sh
#
# Release with extra cargo flags
# EXTRA_ARGS="--no-default-features" ./build_release.sh
#
# Release using custom builder eg cargo
# BUILDER=cargo ./build_release.sh

set -eou pipefail

DEFAULT_TARGET="$(rustup target list | grep '(default)' | awk '{print $1}')"
ARGS=${EXTRA_ARGS:-}

APP_NAME=vector
BUILDER_COMMAND=${BUILDER:-"cross"}

if [ -z "$TARGET" ]; then
    echo "TARGET is not passed using $DEFAULT_TARGET"
    TARGET="$DEFAULT_TARGET"
fi

ROOT_DIR="$(pwd)"
DIST_DIR="$ROOT_DIR/dist"
RELEASE_DIR="$DIST_DIR/$APP_NAME-$VERSION"
BIN_DIR="$RELEASE_DIR/bin"
CONFIG_DIR="$RELEASE_DIR/config"
BINARY_PATH="$ROOT_DIR/target/$TARGET/release/$APP_NAME"
TAR_NAME="$APP_NAME-$VERSION-$TARGET.tar.gz"

function build_release() {
  $BUILDER_COMMAND build --target $TARGET --release $ARGS
  mkdir -p $BIN_DIR
  cp "$BINARY_PATH" "$BIN_DIR"
}

function copy_files() {
  cp -r config $CONFIG_DIR
  cp README.md $RELEASE_DIR
  cp LICENSE $RELEASE_DIR
}

function build_tar() {
  tar cvpf $TAR_NAME $DIST_DIR
  rm -rf $RELEASE_DIR
}

build_release
copy_files
build_tar
