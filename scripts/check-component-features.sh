#!/bin/bash
set -euo pipefail

# check-component-features.sh
#
# SUMMARY
#
#   Ensures that each component feature in `Cargo.toml` declares all dependencies
#   necessary to build it without having other features enabled.

cd $(dirname $0)/..

cat Cargo.toml |
  remarshal --if toml --of json |
  jq -r '.features.sources,.features.transforms,.features.sinks|.[]' |
  xargs -I{} sh -cx 'cargo check --tests --no-default-features --features {} || exit 255'
