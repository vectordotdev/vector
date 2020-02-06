#!/usr/bin/env bash

# run.sh
#
# SUMMARY
#
#   A simple script that runs commands in a Docker environment based on the
#   presence of the `USE_DOCKER` environment variable.
#
#   This helps to reduce the friction running various `make` commands. For
#   example, the `make generate` command requires Ruby and other dependencies
#   to be installed. Routing this command through a Docker images removes this.

set -eou pipefail

if [ "$USE_DOCKER" == "true" ]; then
  echo "Executing within Docker. To disable set USE_DOCKER to false"
  echo ""
  echo "  make ... USE_DOCKER=false"
  echo ""

  scripts/docker-run.sh "$@"
else
  echo "Executing locally. To use Docker set USE_DOCKER to true"
  echo ""
  echo "  make ... USE_DOCKER=true"
  echo ""

  ${@:2}
fi
