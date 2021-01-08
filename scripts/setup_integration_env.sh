#!/usr/bin/env bash
set -o pipefail

# setup_integration_env.sh
#
# SUMMARY
#
#  Sets up Vector integration test environments

set -x

# Echo usage if something isn't right.
usage() {
    echo "Usage: $0 [-i Name of integration suite ] [-a Action to run {stop|start} ] [-t The container tool to use]" 1>&2; exit 1;
}

while getopts i:a:t: o;
do
    case "${o}" in
        i) INTEGRATION=${OPTARG};;
        a) ACTION=${OPTARG};;
        :)
        echo "ERROR: Option -$OPTARG requires an argument"
        usage
        ;;
        *)
        echo "ERROR: Invalid option -$OPTARG"
        usage
        ;;
    esac
done
shift $((OPTIND-1))

# Check required switches exist
if [ -z "${INTEGRATION}" ] || [ -z "${ACTION}" ]; then
  usage
fi

INTEGRATION="${INTEGRATION:-"none"}"
ACTION="${ACTION:-"stop"}"

echo "Setting up Test Integration environment for ${INTEGRATION}..."

(  ./scripts/setup_integration/"${INTEGRATION}"_integration_env.sh "${ACTION}" )
