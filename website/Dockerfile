# Node.js and Yarn are supplied by the pinned image.
FROM node:24-bookworm-slim@sha256:873f586f3794f52281bd039bf02c048a0be08fde619338f9941bad1eb6b34530

ARG HUGO_VERSION=0.154.5
ARG CUE_VERSION=0.17.1

ENV PATH="/root/.cargo/bin:/root/.local/bin:${PATH}"

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl git libssl-dev make pkg-config unzip \
    && rm -rf /var/lib/apt/lists/*

# Hugo extended (required for Sass/PostCSS processing)
# uname -m: x86_64 -> amd64, aarch64 -> arm64
RUN ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') && \
    curl -fsSL https://github.com/gohugoio/hugo/releases/download/v${HUGO_VERSION}/hugo_extended_${HUGO_VERSION}_linux-${ARCH}.tar.gz \
    | tar -xz -C /usr/local/bin hugo

# CUE CLI
RUN ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') && \
    curl -fsSL https://github.com/cue-lang/cue/releases/download/v${CUE_VERSION}/cue_v${CUE_VERSION}_linux_${ARCH}.tar.gz \
    | tar -xz -C /usr/local/bin cue

WORKDIR /app/website

EXPOSE 1313

# `prepare.sh` installs the matching vdev release or builds the checkout when it is unreleased.
ENTRYPOINT ["/bin/bash", "-lc", \
  "cd /app && git config --global --replace-all safe.directory /app && \
   scripts/environment/prepare.sh --modules=vdev,prettier,protoc && \
   make generate-docs && \
   cd /app/website && \
   yarn && \
   cp -f ../Cargo.lock data/cargo-lock.toml && \
   make serve-development SERVER_BIND=0.0.0.0 SERVER_PORT=1313"]
