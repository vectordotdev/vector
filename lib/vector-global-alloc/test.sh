#!/bin/bash
set -euo pipefail

# This file runs the tests for all the interesting feature combinations.

expect-fail() {
  ! "$@" || {
    { set +x; } &> /dev/null
    echo >&2 "Expected $* to fail, but it worked";
    exit 1;
  }

  # Print a note that this fail is expected.
  { set +x; } &> /dev/null
  echo "^^^ command failed *as expected* ^^^"
  set -x
}

set -x

cargo test --features system
cargo test --features jemalloc
expect-fail cargo check
expect-fail cargo check --no-default-features
