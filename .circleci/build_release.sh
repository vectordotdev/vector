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

set -eo pipefail

# Args

DEFAULT_TARGET="$(rustup target list | grep '(default)' | awk '{print $1}')"
ARGS=${EXTRA_ARGS:-}

# Defaults

if [ -z "$TARGET" ]; then
    echo "TARGET is not passed using $DEFAULT_TARGET"
    TARGET="$DEFAULT_TARGET"
fi

if [ -z "$VERSION" ]; then
    echo "VERSION is not passed using version.sh as the default"
    VERSION=$(./.circleci/version.sh)
fi

set -u

# Variables

APP_NAME=vector
BUILDER_COMMAND=${BUILDER:-"cross"}
ROOT_DIR="$(pwd)"
DIST_DIR="$ROOT_DIR/dist"
RELEASE_DIR_NAME="$APP_NAME-$VERSION"
RELEASE_DIR="$DIST_DIR/$RELEASE_DIR_NAME"
BIN_DIR="$RELEASE_DIR/bin"
CONFIG_DIR="$RELEASE_DIR/config"
BINARY_PATH="$ROOT_DIR/target/$TARGET/release/$APP_NAME"
TAR_NAME="$APP_NAME-$VERSION-$TARGET.tar.gz"

# Functions

function build_release() {
  $BUILDER_COMMAND build --target $TARGET --release $ARGS
  mkdir -p $BIN_DIR
  cp "$BINARY_PATH" "$BIN_DIR"
  echo "Copied $BINARY_PATH to $BIN_DIR"
}

function copy_files() {
  cp -r config $CONFIG_DIR
  echo "Copied config to $CONFIG_DIR"

  cp README.md $RELEASE_DIR
  echo "Copied README.md to $RELEASE_DIR"

  cp LICENSE $RELEASE_DIR
  echo "Copied LICENSE to $RELEASE_DIR"
}

function build_tar() {
  cd $DIST_DIR
  rm -rf $TAR_NAME
  tar cvpf $TAR_NAME $RELEASE_DIR_NAME
  echo "Built tar located at $(pwd)/$TAR_NAME"
  rm -rf $RELEASE_DIR_NAME
  cd $ROOT_DIR
}

# Execute

build_release
copy_files
build_tar
