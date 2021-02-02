#!/usr/bin/env bash
set -euo pipefail

# check-component-features.sh
#
# SUMMARY
#
#   Ensures that all examples are valid

for config in ./config/examples/*.toml ; do
  cargo run -- validate --deny-warnings --no-environment "$config"
done
