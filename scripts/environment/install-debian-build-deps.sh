#!/usr/bin/env bash
set -euo pipefail

# Canonical apt-get build dependencies for Vector's Debian-based builder images.

apt-get update
apt-get install -y --no-install-recommends \
  build-essential \
  clang \
  cmake \
  curl \
  git \
  libclang-dev \
  libsasl2-dev \
  libssl-dev \
  libxxhash-dev \
  mold \
  odbcinst \
  odbc-mariadb \
  odbc-postgresql \
  perl \
  pkg-config \
  unixodbc \
  unixodbc-dev \
  unzip \
  zlib1g-dev
rm -rf /var/lib/apt/lists/*
