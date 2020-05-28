#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-"all"}"

GIT_DIR="$(git rev-parse --git-dir)"

mkdir -p "$GIT_DIR/hooks"

if [[ "$MODE" == "all" || "$MODE" == "signoff" ]]; then
  cp scripts/signoff-git-hook.sh "$GIT_DIR/hooks/commit-msg"
fi
