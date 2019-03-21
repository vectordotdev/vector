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
VERSION="$(git describe --abbrev=0 --tags)"

if [ -z "$TARGET" ]; then
    echo "TARGET is not passed using $DEFAULT_TARGET"
    TARGET="$DEFAULT_TARGET"
fi

S3_PREFIX="s3://packages.timber.io/vector"
TAR_NAME="$APP_NAME-$VERSION-$TARGET.tar.gz"

BUILDER_COMMAND=${BUILDER:-"cross"}

function build_release() {
    $BUILDER_COMMAND build --target $TARGET --release $ARGS --frozen
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
    S3_PATH="$S3_PREFIX/$TAR_NAME"
    aws s3 cp "$DIST_DIR/$TAR_NAME" "$S3_PATH"
}

build_release
build_tar
upload_s3
