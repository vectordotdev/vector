ARG RUST_VERSION=1
ARG FEATURES
ARG BUILD_TESTS=false

FROM docker.io/rust:${RUST_VERSION}-slim-trixie

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    curl \
    git \
    clang \
    libclang-dev \
    libsasl2-dev \
    libssl-dev \
    libxxhash-dev \
    zlib1g-dev \
    zlib1g \
    unzip \
    mold \
  && rm -rf /var/lib/apt/lists/*

COPY tests/data/ca/certs /certs

COPY scripts/environment/install-protoc.sh /
COPY scripts/environment/prepare.sh /
COPY scripts/environment/binstall.sh /
COPY scripts/environment/release-flags.sh /

WORKDIR /vector
COPY rust-toolchain.toml .

RUN bash /prepare.sh --modules=cargo-nextest
RUN bash /install-protoc.sh

COPY . .
ARG FEATURES
ARG BUILD_TESTS

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    if [ "$BUILD_TESTS" = "true" ]; then \
        mkdir -p /home/target/debug && \
        CARGO_BUILD_TARGET_DIR=/home/target /usr/bin/mold -run cargo build --tests --lib --bin vector \
        --no-default-features --features $FEATURES && \
        cp /home/target/debug/vector /usr/bin/vector; \
    fi
