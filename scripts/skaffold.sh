#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

triple="$(rustc --version --verbose | grep host | awk '{ print $2 }')"

# Initial vector build to ensure we start at a valid state.
if [[ "x86_64-unknown-linux-gnu" == "$triple" ]] ; then
  cargo build
  mkdir -p target/x86_64-unknown-linux-gnu/debug
  cp target/debug/vector target/x86_64-unknown-linux-gnu/debug/vector
else
  PROFILE=debug make target/x86_64-unknown-linux-gnu/vector
fi

# Prepare .dockerignore so we don't send the whole dir to the docker as the
# context.
scripts/skaffold-dockerignore.sh

# Watch for changes in he background and rebuild the vector binary.
cargo watch -x build &

# Kill all child processes of this bash instance.
trap 'kill -- "-$$"; exit 0' EXIT

export SKAFFOLD_CACHE_ARTIFACTS=false
skaffold "$@"
