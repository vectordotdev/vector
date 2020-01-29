#!/usr/bin/env bash

# run.sh
#
# SUMMARY
#
#   A simple utility script that run the passed command in a Docker environment
#   or locally based on the presence of the `USE_DOCKER` environment variable.
#
#   * If set to `false` then all `make` targets will execute on the local
#     machine.
#   * If set to `true` then all `make` targets will execute in their respective
#     Docker images.
#
#   We offer this to reduce the user frustration when running `make` command.
#   For example, `make generate` requires the user to isntall Ruby and other
#   dependencies. Docker makes this process much simpler for new contributors.

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
