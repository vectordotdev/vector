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
BUILDTIME=$(date +%Y-%m-%d.%s)

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
  cd ..
}

function upload_s3() {
  S3_URI="s3://packages.timber.io/vector/$S3_PATH$TAR_NAME"
  aws s3 cp "$DIST_DIR/$TAR_NAME" "$S3_URI"
}

function build_and_upload() {
  build_release
  build_tar
  upload_s3
}

# Temporarily allow unset variables in order to construct the BUILDSTAMP based
# on variables that only _might_ be set
set +u

TAG=$CIRCLE_TAG
BRANCH=$CIRCLE_BRANCH

if [ -n "$TAG" ]
then
  echo "Building release for tag $TAG"

  S3_PATH="tags/$TAG/"
  TAR_NAME="$APP_NAME-$TAG-$TARGET.tar.gz"
  build_and_upload
elif [ -n "$BRANCH" ]
then
  S3_PATH="branches/$BRANCH/"
  TAR_NAME="$APP_NAME-$BRANCH-$CIRCLE_SHA1-$TARGET.tar.gz"
  build_and_upload

  S3_PATH="branches/$BRANCH/"
  TAR_NAME="$APP_NAME-$BRANCH-latest-$TARGET.tar.gz"
  build_and_upload
else
  echo "error: neither TAG nor BRANCH was set"
  exit 1
fi
set -u
