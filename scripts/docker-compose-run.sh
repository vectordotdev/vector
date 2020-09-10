#!/usr/bin/env bash
set -euo pipefail

# docker-compose-run.sh
#
# SUMMARY
#
#   Runs a job from `docker-compose.yml` file.

SERVICE="$1"

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# A workaround to prevent docker from creating directories at `./target` as
# root.
# Ran unconditionally for consistency between docker and bare execution.
scripts/prepare-target-dir.sh

USER="$(id -u):$(id -g)"
export USER

docker-compose rm -svf "$SERVICE" 2>/dev/null || true
docker-compose up --build --abort-on-container-exit --exit-code-from "$SERVICE" "$SERVICE" \
  | sed $'s/^.*container exit...$/\033[0m\033[1A/'
