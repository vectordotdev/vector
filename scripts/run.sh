#!/usr/bin/env bash
set -euo pipefail

# run.sh
#
# SUMMARY
#
#   A simple script that runs a make target in a container environment based
#   on the presence of the `USE_CONTAINER` environment variable.
#
#   This helps to reduce friction for first-time contributors, since running
#   basic commands through containers ensures they work. It is recommended
#   that frequent contributors setup local environments to improve the speed
#   of commands they are running frequently. This can be achieved by setting
#   USE_CONTAINER to none:
#
#       export USE_CONTAINER=none
#

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# A workaround to prevent docker from creating directories at `./target` as
# root.
# Ran unconditionally for consistency between docker and bare execution.
scripts/prepare-target-dir.sh

case "$USE_CONTAINER" in
  docker | podman)
    echo "Executing within $USE_CONTAINER. To disable set USE_CONTAINER to none"
    echo ""
    echo "  make ... USE_CONTAINER=none"
    echo ""

    scripts/docker-compose-run.sh "$1"
    ;;

  *)
    echo "Executing locally. To use Docker set USE_CONTAINER to docker or podman"
    echo ""
    echo "  make ... USE_CONTAINER=docker"
    echo "  make ... USE_CONTAINER=podman"
    echo ""

    FILE=$(find ./scripts -name "${1}.*")

    if [ -z "$FILE" ]; then
      echo "Local invocation failed. Script not found!"
      echo ""
      echo "    scripts/${1}.*"
      echo ""
      echo "To run the ${1} target locally you must place a script in the"
      echo "/scripts folder that can be executed. Otherwise, you can use the"
      echo "service defined in /docker-compose.yml."
      exit 1
    fi

    ${FILE}
    ;;
esac
