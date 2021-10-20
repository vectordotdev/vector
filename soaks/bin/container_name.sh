#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOAK_ROOT="${__dir}/.."

display_usage() {
	echo -e "\nUsage: \$0 SOAK_NAME COMMIT_SHA\n"
}

if [  $# -le 1 ]
then
    display_usage
    exit 1
fi

SOAK_NAME="${1:-}"
COMMIT_SHA="${2:-}"

# shellcheck disable=SC1091
. "${SOAK_ROOT}/${SOAK_NAME}/FEATURES"

FEATURE_SHA=$(echo -n "${FEATURES}" | sha256sum - | head -c40)
IMAGE="vector:${COMMIT_SHA}-${FEATURE_SHA}"

echo "${IMAGE}"
