#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o verbose

# Set VECTOR_BUILD_DESC to add in pertinent build information.  We typically only
# enable this when generating binaries that will be in the hands of users so that
# we know which Git commit it was built from, etc.
GIT_SHA=$(git rev-parse --short HEAD)
CURRENT_DATE=$(date +%Y-%m-%d)
BUILD_DESC="${GIT_SHA} ${CURRENT_DATE}"

# If we're in Github CI, set it in the special environment variables file. Otherwise,
# export the variable.  This requires sourcing the file instead of simply running it.
if [[ -f "${GITHUB_ENV}" ]]; then
    echo VECTOR_BUILD_DESC="${BUILD_DESC}" >> "${GITHUB_ENV}"
else
    export VECTOR_BUILD_DESC="${BUILD_DESC}"
fi
