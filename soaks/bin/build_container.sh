#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

# We need to build two copies of vector with the same flags, one for the
# baseline SHA and the other for current. Baseline is either 'master' or
# whatever the user sets.

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOAK_ROOT="${__dir}/.."
PATCH_DIR="${SOAK_ROOT}/patches"

display_usage() {
	echo -e "\nUsage: \$0 SOAK_NAME COMMIT_SHA\n"
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
        git apply "${PATCH_DIR}/blank_global_dockerfileignore.patch"
        popd
    fi

    docker build --file "${SOAK_ROOT}/Dockerfile" --build-arg=VECTOR_FEATURES="${FEATURES}" --tag "${IMAGE}" "${BUILD_DIR}"
    rm -rf "${BUILD_DIR}"
}

if [  $# -le 1 ]
then
    display_usage
    exit 1
fi

SOAK_NAME="${1:-}"
COMMIT_SHA="${2:-}"

SOAK_DIR="${SOAK_ROOT}/${SOAK_NAME}"
# Shellcheck cannot follow dynamic paths properly.
# shellcheck disable=SC1091
. "${SOAK_DIR}/FEATURES"
IMAGE=$(./bin/container_name.sh "${SOAK_NAME}" "${COMMIT_SHA}")
docker image inspect "${IMAGE}" > /dev/null || build_vector "${IMAGE}" "${COMMIT_SHA}"
