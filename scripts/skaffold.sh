#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Inital vector build to ensure we start at a valid state.
cargo build

# Prepare .dockerignore so we don't send the whole dir to the docker as the
# context.
cat <<EOF >target/debug/.dockerignore
**/*
!vector
EOF

# Configure docker (and skaffold) to use minikube docker host.
# shellcheck source=minikube-docker-env.sh disable=SC1091
. scripts/minikube-docker-env.sh

# Watch for changes in he background and rebuild the vector binary.
cargo watch -x build &

# Kill all child processes of this bash instance.
trap 'kill -- "-$$"; exit 0' EXIT

export SKAFFOLD_CACHE_ARTIFACTS=false
skaffold "$@"
