#!/usr/bin/env bash
set -uo pipefail

# setup_integration_env.sh
#
# SUMMARY
#
#  Sets up Vector integration test environments

set -x

while getopts i:a:t: flag
do
    case "${flag}" in
        i) integration=${OPTARG};;
        a) action=${OPTARG};;
        t) tool=${OPTARG};;
    esac
done

INTEGRATION="${integration:-"none"}"
ACTION="${action:-"stop"}"

case $CONTAINER_TOOL in
  "podman")
    CONTAINER_ENCLOSURE="pod"
    ;;
  "docker")
    CONTAINER_ENCLOSURE="network"
    ;;
  *)
    CONTAINER_ENCLOSURE="unknown"
    ;;
esac

echo "Setting up Test Integration environment for ${INTEGRATION}..."

(  ./scripts/setup_integration/${INTEGRATION}_integration_env.sh -a $ACTION -t  $CONTAINER_TOOL -e $CONTAINER_ENCLOSURE  )
