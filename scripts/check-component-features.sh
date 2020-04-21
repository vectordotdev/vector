#!/bin/bash
set -euo pipefail

# check-component-features.sh
#
# SUMMARY
#
#   Ensures that all components have corresponding features in `Cargo.toml` and
#   that each of these features declares declares all dependencies
#   necessary to build it without having other features enabled.

cd $(dirname $0)/..

echo "Checking that Vector and tests can be built without default features..."
cargo check --tests --no-default-features

echo "Checking that all components have corresponding features in Cargo.toml..."
components=$(cargo run --no-default-features -- list)
if (echo "$components" | egrep -v "^(Sources:|Transforms:|Sinks:|)$" >/dev/null); then
  echo "Some of the components do not have a corresponding feature flag in Cargo.toml:"
  echo "$components" | sed "s/^/    /"
  exit 1
fi

echo "Checking that each source feature can be built without other features..."
cat Cargo.toml |
  remarshal --if toml --of json |
  jq -r ".features.sources|.[]" |
  xargs -I{} sh -cx "(cargo check --tests --no-default-features --features {}) || exit 255"

if (${CI:-false}); then
  echo "Cleaning to save some disk space"
  cargo clean
fi

echo "Checking that each transform feature can be built without other features..."
cat Cargo.toml |
  remarshal --if toml --of json |
  jq -r ".features.transforms|.[]" |
  xargs -I{} sh -cx "(cargo check --tests --no-default-features --features {}) || exit 255"

if (${CI:-false}); then
  echo "Cleaning to save some disk space"
  cargo clean
fi

echo "Checking that each sink feature can be built without other features..."
cat Cargo.toml |
  remarshal --if toml --of json |
  jq -r ".features.sinks|.[]" |
  xargs -I{} sh -cx "(cargo check --tests --no-default-features --features {}) || exit 255"
