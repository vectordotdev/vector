#!/usr/bin/env bash
set -euo pipefail

# Build and install librdkafka to a per-version prefix under $HOME.
# Used by CI (cached via actions/cache) and locally by developers whose
# distro ships a librdkafka older than $LIBRDKAFKA_VERSION.
#
# Required at build time (CI installs these on cache miss):
#   libsasl2-dev zlib1g-dev libssl-dev libzstd-dev libcurl4-openssl-dev
#
# Env:
#   LIBRDKAFKA_VERSION  librdkafka version to install (default: 2.12.1)
#   LIBRDKAFKA_PREFIX   install prefix (default: $HOME/.local/librdkafka-$LIBRDKAFKA_VERSION)

LIBRDKAFKA_VERSION="${LIBRDKAFKA_VERSION:-2.12.1}"
LIBRDKAFKA_PREFIX="${LIBRDKAFKA_PREFIX:-$HOME/.local/librdkafka-$LIBRDKAFKA_VERSION}"

if [ -f "$LIBRDKAFKA_PREFIX/lib/pkgconfig/rdkafka.pc" ]; then
  echo "librdkafka $LIBRDKAFKA_VERSION already installed at $LIBRDKAFKA_PREFIX"
  exit 0
fi

TEMP="$(mktemp -d)"
trap 'rm -rf "$TEMP"' EXIT

echo "Downloading librdkafka $LIBRDKAFKA_VERSION"
curl -fsSL \
  "https://github.com/confluentinc/librdkafka/archive/refs/tags/v${LIBRDKAFKA_VERSION}.tar.gz" \
  -o "$TEMP/librdkafka.tar.gz"
tar -xzf "$TEMP/librdkafka.tar.gz" -C "$TEMP"

cd "$TEMP/librdkafka-${LIBRDKAFKA_VERSION}"

echo "Configuring librdkafka with prefix=$LIBRDKAFKA_PREFIX"
./configure --prefix="$LIBRDKAFKA_PREFIX"

if command -v nproc >/dev/null 2>&1; then
  JOBS="$(nproc)"
else
  JOBS=2
fi

echo "Building librdkafka with -j$JOBS"
make -j"$JOBS"
make install

echo "Installed librdkafka $LIBRDKAFKA_VERSION to $LIBRDKAFKA_PREFIX"
