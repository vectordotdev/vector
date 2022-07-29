#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd $(dirname "$BASH_SOURCE[0]") && pwd)" &> /dev/null
LLVM_VERSION=$(llvm-config --version)
LLVM_VERSION_MAJOR=$(echo "$LLVM_VERSION" | cut -d '.' -f1)
RUSTC_LLVM_VERSION=$(rustc --version --verbose | grep LLVM | cut -d ' ' -f3)
RUSTC_LLVM_VERSION_MAJOR=$(echo "$RUSTC_LLVM_VERSION" | cut -d '.' -f1)

echo Building precompiled-sys with configuration:
echo TARGET="$TARGET"
echo PROFILE="$PROFILE"
echo LLVM_VERSION="$LLVM_VERSION"
echo RUSTC_LLVM_VERSION="$RUSTC_LLVM_VERSION"
echo

PROFILE_ARG=""
if [ "$PROFILE" = "release" ]; then
  PROFILE_ARG="--release"
fi
if ! [ "$LLVM_VERSION_MAJOR" = "$RUSTC_LLVM_VERSION_MAJOR" ]; then
  >&2 echo "LLVM version \""$LLVM_VERSION"\" does not match major rustc LLVM version \""$RUSTC_LLVM_VERSION"\""
  exit 1
fi

PRECOMPILED_DIR="$SCRIPT_DIR"
PRECOMPILED_TARGET_DIR="$PRECOMPILED_DIR"/target
PRECOMPILED_BUILD_DIR="$PRECOMPILED_TARGET_DIR"/"$TARGET"/"$PROFILE"

RUSTFLAGS="--emit=llvm-bc -C relocation-model=pic -C codegen-units=1 -C linker-plugin-lto" RUSTC_BOOTSTRAP=1 cargo build --manifest-path="$PRECOMPILED_DIR"/Cargo.toml $PROFILE_ARG --lib --target "$TARGET" --target-dir="$PRECOMPILED_TARGET_DIR" -Z build-std=std

BC_EXCLUDE_PATTERN="(panic_abort|proc_macro).*\.bc"
BC_FILES=$(ls "$PRECOMPILED_BUILD_DIR"/deps/*.bc | egrep -v -i "$BC_EXCLUDE_PATTERN")

llvm-link $BC_FILES > "$PRECOMPILED_BUILD_DIR"/precompiled.bc
