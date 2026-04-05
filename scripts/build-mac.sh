#!/usr/bin/env bash
# Build Vector binaries for macOS (arm64 or x86_64) from the current branch.
# Usage: ./scripts/build-mac.sh
# Requires: Rust 1.88+ (use rustup and install 1.92: rustup toolchain install 1.92)
# Ensure rustup's cargo is in PATH: export PATH="$HOME/.cargo/bin:$PATH"

set -euo pipefail

cd "$(dirname "$0")/.."
REPO_ROOT="${PWD}"

# Prefer rustup's cargo so rust-toolchain.toml is respected (Vector needs Rust 1.88+)
export PATH="${HOME}/.cargo/bin:${PATH}"

# Detect Mac architecture
ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
  TARGET="arm64-apple-darwin"
elif [ "$ARCH" = "x86_64" ]; then
  TARGET="x86_64-apple-darwin"
else
  echo "Unsupported architecture: $ARCH"
  exit 1
fi

echo "Building Vector for $TARGET ..."

# Use rustup toolchain if rust-toolchain.toml exists (ensures correct Rust version)
export TARGET
export NATIVE_BUILD=true

# Build only the vector binary (avoids building vdev which may need nightly features)
cargo build -p vector --release --no-default-features --features default

# Version for the archive (from Cargo.toml if vdev not available)
VECTOR_VERSION="${VECTOR_VERSION:-$(grep '^version' Cargo.toml | head -1 | sed 's/.*= *"\(.*\)".*/\1/' | tr -d ' ')}"
export VECTOR_VERSION

echo "Creating package archive ..."
bash scripts/package-archive.sh

ARCHIVE="target/artifacts/vector-${VECTOR_VERSION}-${TARGET}.tar.gz"
echo ""
echo "Done. Binary: target/release/vector"
echo "Archive: $ARCHIVE"
if [ -f "$ARCHIVE" ]; then
  ls -la "$ARCHIVE"
fi
