#!/bin/bash
set -euo pipefail

# docker-compose-run.sh
#
# SUMMARY
#
#   Runs a job from `docker-compose.yml` file.

SERVICE="$1"

DOCKER="${USE_CONTAINER:-"docker"}"
COMPOSE="${COMPOSE:-"${DOCKER}-compose"}"
DOCKER_IMAGES_STRATEGY="${DOCKER_IMAGES_STRATEGY:-"build"}"

ARGS=()
case "$DOCKER_IMAGES_STRATEGY" in
  build)
    ARGS+=(--build)
    ;;
  pull)
    ARGS+=(--no-build)
    $COMPOSE pull "$SERVICE"
    ;;
  default)
    # No custom args
    ;;
  *)
    echo "Error: invalid DOCKER_IMAGES_STRATEGY: $DOCKER_IMAGES_STRATEGY" >&2
    exit 1
    ;;
esac

USER="$(id -u):$(id -g)"
export USER

$COMPOSE rm -svf "$SERVICE" 2>/dev/null || true

$COMPOSE up \
  "${ARGS[@]}" \
  --abort-on-container-exit \
   --exit-code-from "$SERVICE" \
   "$SERVICE" \
  | sed $'s/^.*container exit...$/\033[0m\033[1A/'
