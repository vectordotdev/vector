ARG RUST_VERSION
FROM rust:${RUST_VERSION}-slim-bullseye

RUN apt-get update \
  && apt-get install -y g++ cmake pkg-config libclang1-9 llvm-9 libsasl2-dev libssl-dev \
  && rm -rf /var/lib/apt/lists/*
