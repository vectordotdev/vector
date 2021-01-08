#!/usr/bin/env bash
set -o pipefail

# setup_integration_env.sh
#
# SUMMARY
#
#  Sets up Vector integration test environments

set -x

if [ $# -ne 2 ]
then
    echo "Usage: $0 {integration_test_suite} {stop|start}" 1>&2; exit 1;
    exit 1
fi
INTEGRATION=$1
ACTION=$2

echo "Setting up Test Integration environment for ${INTEGRATION}..."

(  ./scripts/setup_integration/"${INTEGRATION}"_integration_env.sh "${ACTION}" )
