#!/usr/bin/env bash
set -euo pipefail

# docker-compose-run.sh
#
# SUMMARY
#
#   Runs a job from `docker-compose.yml` file.

SERVICE="$1"

DOCKER="${USE_CONTAINER:-"docker"}"
COMPOSE="${COMPOSE:-"${DOCKER}-compose"}"

USER="$(id -u):$(id -g)"
export USER

$COMPOSE rm -svf "$SERVICE" 2>/dev/null || true
$COMPOSE up --build --abort-on-container-exit --exit-code-from "$SERVICE" "$SERVICE" \
  | sed $'s/^.*container exit...$/\033[0m\033[1A/'
