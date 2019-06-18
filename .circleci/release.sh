#!/usr/bin/env bash

# Usage
#
# Release with cross via the default system target
# ./release.sh
#
# Release with the specified target
# TARGET="<my target>" ./release.sh
#
# Release with extra cargo flags
# EXTRA_ARGS="--no-default-features" ./release.sh
#
# Release using custom builder eg cargo
# BUILDER=cargo ./release.sh

set -eou pipefail

DEFAULT_TARGET="$(rustup target list | grep '(default)' | awk '{print $1}')"
ARGS=${EXTRA_ARGS:-}

APP_NAME=vector
DIST_DIR="$(pwd)/dist"
ROOT_DIR="$(pwd)"

if [ -z "$TARGET" ]; then
    echo "TARGET is not passed using $DEFAULT_TARGET"
    TARGET="$DEFAULT_TARGET"
fi

BUILDER_COMMAND=${BUILDER:-"cross"}

function build_release() {
  $BUILDER_COMMAND build --target $TARGET --release $ARGS
}

function build_tar() {
  mkdir -p $DIST_DIR
  cp "target/$TARGET/release/$APP_NAME" "$DIST_DIR"
  cd $DIST_DIR
  tar cvpf $TAR_NAME $APP_NAME
  rm $APP_NAME
  cd $ROOT_DIR
}

function copy_config() {
  cp -r config $DIST_DIR/config
}

# Temporarily allow unset variables in order to construct the BUILDSTAMP based
# on variables that only _might_ be set
set +u

TAG=$CIRCLE_TAG
BRANCH=$CIRCLE_BRANCH
COMMIT_SHA=$CIRCLE_SHA1
COMMIT_TIMESTAMP=$(git show -s --format=%ct $COMMIT_SHA)
VERSION=$(./.circleci/version.sh)

echo "Building release for $VERSION"

TAR_NAME="$APP_NAME-$VERSION-$TARGET.tar.gz"
build_release
build_tar
copy_config

set -u
