#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOAK_ROOT="${__dir}/.."
PATCH_DIR="${SOAK_ROOT}/patches"

display_usage() {
    echo ""
    echo "Usage: $0 COMMIT_SHA IMAGE_NAME"
}

build_vector() {
    local IMAGE="${1}"
    local SHA="${2}"
    local BUILD_DIR="/tmp/vector-${SHA}"

    if [ ! -d "${BUILD_DIR}" ]; then
        mkdir "${BUILD_DIR}"
        pushd "${BUILD_DIR}"
        git init
        git remote add origin https://github.com/vectordotdev/vector.git
        git fetch --depth 1 origin "${SHA}"
        git checkout FETCH_HEAD
        popd
    fi

    docker build --file "${SOAK_ROOT}/Dockerfile" --tag "${IMAGE}" "${BUILD_DIR}"
    rm -rf "${BUILD_DIR}"
}

if [  $# -le 1 ]
then
    display_usage
    exit 1
fi

COMMIT_SHA="${1:-}"
IMAGE="${2:-}"

docker image inspect "${IMAGE}" > /dev/null || build_vector "${IMAGE}" "${COMMIT_SHA}"
