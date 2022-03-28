#!/usr/bin/env bash
set -o pipefail

# setup_integration_env.sh
#
# SUMMARY
#
#  Sets up Vector integration test environments
if [ $# -ne 2 ]
then
    echo "Usage: $0 {integration_test_suite} {stop|start}" 1>&2; exit 1;
    exit 1
fi
INTEGRATION=$1
ACTION=$2

# Check container tool and default to podman
if [ -z "${CONTAINER_TOOL}" ]; then
	echo "Container tool is unset, defaulting to podman"
	export CONTAINER_TOOL="podman"
else
	echo "Container tool is ${CONTAINER_TOOL}..."
fi

echo "Setting up Test Integration environment for ${INTEGRATION}..."

exec ./scripts/setup_integration/"${INTEGRATION}"_integration_env.sh "${ACTION}"
