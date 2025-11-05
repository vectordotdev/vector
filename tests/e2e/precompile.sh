#!/bin/bash
# ============================================================================
# Precompile Script for Vector Test Runner
# ============================================================================
# Precompiles integration and e2e tests with specified features.
# Only copies runtime artifacts (not incremental build cache).
# ============================================================================

set -euo pipefail

PRECOMPILE="${1:-false}"
FEATURES="${2:-}"

if [ "$PRECOMPILE" != "true" ]; then
    echo "==> Skipping test precompilation (PRECOMPILE=false)"
    exit 0
fi

echo "==> Compiling tests with features: $FEATURES"

# Compile with mold linker for faster builds
CARGO_BUILD_TARGET_DIR=/home/target \
/usr/bin/mold -run cargo build \
    --tests \
    --lib \
    --bin vector \
    --no-default-features \
    --features "$FEATURES"

echo "==> Installing vector binary to /usr/bin/vector"
cp /home/target/debug/vector /usr/bin/vector

echo "==> Cleanup: Removing non-essential build artifacts to save space"
# Remove .fingerprint (cargo metadata, not needed at runtime)
rm -rf /home/target/debug/.fingerprint

# Remove examples (not needed for tests)
rm -rf /home/target/debug/examples

echo "==> Copying test artifacts to /precompiled-target"
mkdir -p /precompiled-target/debug

# Copy only test executables (not all of deps/)
echo "  Copying test binaries..."
find /home/target/debug/deps -type f -executable -name "*-*" ! -name "*.so" ! -name "*.rlib" -exec cp {} /precompiled-target/debug/ \;

# Copy the vector binary
cp /home/target/debug/vector /precompiled-target/debug/

echo "==> Compilation complete"
