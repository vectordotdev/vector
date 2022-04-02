ARG RUST_VERSION
FROM rust:${RUST_VERSION}-slim-bullseye

RUN apt-get update \
  && apt-get install -y g++ cmake pkg-config libclang1-9 llvm-9 libsasl2-dev libssl-dev zlib1g-dev \
  && rm -rf /var/lib/apt/lists/*

RUN rustup run ${RUST_VERSION} cargo install cargo-nextest --version 0.9.8
