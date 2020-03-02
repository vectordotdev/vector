#!/usr/bin/env bash

# run.sh
#
# SUMMARY
#
#   A simple script that runs commands in a container environment based
#   on the presence of the `USE_CONTAINER` environment variable.
#
#   This helps to reduce the friction running various `make`
#   commands. For example, the `make generate` command requires Ruby and
#   other dependencies to be installed. Routing this command through a
#   container images removes this.

set -eou pipefail

case "$USE_CONTAINER" in
  docker | podman)
    echo "Executing within $USE_CONTAINER. To disable set USE_CONTAINER to none"
    echo ""
    echo "  make ... USE_CONTAINER=none"
    echo ""

    scripts/docker-run.sh "$@"
    ;;

  *)
    echo "Executing locally. To use Docker set USE_CONTAINER to docker or podman"
    echo ""
    echo "  make ... USE_CONTAINER=docker"
    echo "  make ... USE_CONTAINER=podman"
    echo ""

    ${@:2}
esac
