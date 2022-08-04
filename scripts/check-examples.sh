#!/usr/bin/env bash
set -euo pipefail

# check-examples.sh
#
# SUMMARY
#
#   Ensures that all examples are valid

for config in ./config/examples/* ; do
  if [ -d "$config" ]; then
    cargo run -- validate --deny-warnings --no-environment --config-dir "$config"
  else
    cargo run -- validate --deny-warnings --no-environment "$config"
  fi
done
