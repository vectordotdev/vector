#!/bin/sh
set -o errexit

export DEBIAN_FRONTEND=noninteractive
export ACCEPT_EULA=Y

# Configure apt for speed and efficiency
cat > /etc/apt/apt.conf.d/90-vector-optimizations <<EOF
Acquire::Retries "5";
Acquire::Queue-Mode "host";
Acquire::Languages "none";
APT::Install-Recommends "false";
EOF


apt-get update
apt-get install -y \
  apt-transport-https \
  gnupg \
  wget \
  libclang1 \
  llvm \
  clang \
  unzip \
  libsasl2-dev

# unixODBC development files for the `sources-odbc` feature. Only the
# *-unknown-linux-gnu targets enable it (see the `target-*` feature table in
# Cargo.toml); the musl/arm cross targets omit ODBC so they stay linkable
# without a sysroot libodbc. `odbc-sys` links `libodbc.so` dynamically via
# `#[link(name = "odbc")]`, so the dev package must land in the *target*
# sysroot. For the aarch64 GNU cross build that means the arm64 package, not
# the host amd64 one, so the cross linker can resolve `-lodbc`.
case "${TARGET:-}" in
  x86_64-unknown-linux-gnu)
    apt-get install -y unixodbc-dev
    ;;
  aarch64-unknown-linux-gnu)
    dpkg --add-architecture arm64
    apt-get update
    apt-get install -y unixodbc-dev:arm64
    ;;
esac

