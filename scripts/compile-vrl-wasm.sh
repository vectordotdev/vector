#!/bin/bash
set -euo pipefail

# wasm-compile-vrl.sh
#
# SUMMARY
#
#   Compiles relevant crates to wasm32-unknown-unknown

(
  cd "$(dirname "${BASH_SOURCE[0]}")/../lib/vrl/compiler"
  echo "Compiling lib/vrl/compiler to wasm32-unknown-unknown"
  cargo build --release --target wasm32-unknown-unknown
)

(
  cd "$(dirname "${BASH_SOURCE[0]}")/../lib/vrl/core"
  echo "Compiling lib/vrl/core to wasm32-unknown-unknown"
  cargo build --release --target wasm32-unknown-unknown
)

(
  cd "$(dirname "${BASH_SOURCE[0]}")/../lib/vrl/diagnostic"
  echo "Compiling lib/vrl/diagnostic to wasm32-unknown-unknown"
  cargo build --release --target wasm32-unknown-unknown
)

(
  cd "$(dirname "${BASH_SOURCE[0]}")/../lib/vrl/parser"
  echo "Compiling lib/vrl/parser to wasm32-unknown-unknown"
  cargo build --release --target wasm32-unknown-unknown
)
