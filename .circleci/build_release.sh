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
BUILDER_COMMAND=${BUILDER:-"cross"}
DIST_DIR="$(pwd)/dist"
ROOT_DIR="$(pwd)"

if [ -z "$TARGET" ]; then
    echo "TARGET is not passed using $DEFAULT_TARGET"
    TARGET="$DEFAULT_TARGET"
fi

TAG=$CIRCLE_TAG
BRANCH=$CIRCLE_BRANCH
COMMIT_SHA=$CIRCLE_SHA1
COMMIT_TIMESTAMP=$(git show -s --format=%ct $COMMIT_SHA)
TAR_NAME="$APP_NAME-$VERSION-$TARGET.tar.gz"


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

build_release
copy_config
build_tar
