#!/bin/sh
set -o errexit

apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    libc6-dev \
    cmake \
    curl \
    g++ \
    llvm \
    libclang-dev \
    libsasl2-dev \
    libssl-dev \
    pkg-config \
    zlib1g-dev \
    unzip \
    git \
    golang-go \
  && rm -rf /var/lib/apt/lists/*

rustup run "${RUST_VERSION}" cargo install cargo-nextest --version 0.9.72 --locked
rustup run "${RUST_VERSION}" cargo install bindgen-cli --version 0.71.1 --locked
./install-protoc.sh
