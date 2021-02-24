#!/usr/bin/env bash
set -euo pipefail

# check-component-features.sh
#
# SUMMARY
#
#   Ensures that all components have corresponding features in `Cargo.toml` and
#   that each of these features declares declares all dependencies
#   necessary to build it without having other features enabled.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

toml-extract() {
  WHAT="$1"
  remarshal --if toml --of json | jq -r "$WHAT"
}

extract-features() {
  # The main "features.NAME" only lists categories, so extract from the categories
  for feature in $(toml-extract ".features.\"$1\"|.[]" < Cargo.toml)
  do
    toml-extract ".features.\"$feature\"|.[]" < Cargo.toml
  done | grep "^${1}-" | sort --unique
}

check-listed-features() {
  xargs -I{} sh -cx "(cargo check --tests --no-default-features --features {}) || exit 255"
}

echo "Checking that Vector and tests can be built without default features..."
cargo check --tests --no-default-features

echo "Checking that all components have corresponding features in Cargo.toml..."
COMPONENTS="$(cargo run --no-default-features -- list)"
if (echo "$COMPONENTS" | grep -E -v "(Log level|^(Sources:|Transforms:|Sinks:|)$)" >/dev/null); then
  echo "Some of the components do not have a corresponding feature flag in Cargo.toml:"
  # shellcheck disable=SC2001
  echo "$COMPONENTS" | sed "s/^/    /"
  exit 1
fi

echo "Checking that each source feature can be built without other features..."
extract-features sources | check-listed-features

if (${CI:-false}); then
  echo "Cleaning to save some disk space"
  cargo clean
fi

echo "Checking that each transform feature can be built without other features..."
extract-features transforms | check-listed-features

if (${CI:-false}); then
  echo "Cleaning to save some disk space"
  cargo clean
fi

echo "Checking that each sink feature can be built without other features..."
extract-features sinks | check-listed-features
