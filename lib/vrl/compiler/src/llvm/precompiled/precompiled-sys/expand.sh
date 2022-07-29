#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd $(dirname "$BASH_SOURCE[0]") && pwd)" &> /dev/null

PROFILE_ARG=""
if [ "$PROFILE" = "release" ]; then
  PROFILE_ARG="--release"
fi

PRECOMPILED_DIR="$SCRIPT_DIR"
PRECOMPILED_TARGET_DIR="$PRECOMPILED_DIR"/target
PRECOMPILED_BUILD_DIR="$PRECOMPILED_TARGET_DIR"/"$TARGET"/"$PROFILE"

cargo expand --manifest-path="$PRECOMPILED_DIR"/Cargo.toml $PROFILE_ARG --lib --target "$TARGET" --target-dir="$PRECOMPILED_TARGET_DIR"
