#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-"all"}"

mkdir -p "$(git rev-parse --git-dir)/hooks"

if [[ "$MODE" == "all" || "$MODE" == "signoff" ]]; then
  cp scripts/signoff-git-hook.sh "$(git rev-parse --git-dir)/hooks/commit-msg"
fi
