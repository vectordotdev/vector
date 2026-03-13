ARG RUST_VERSION=1
ARG FEATURES
ARG BUILD=false

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
ARG BUILD

RUN --mount=type=cache,target=/vector/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    if [ "$BUILD" = "true" ]; then \
        /usr/bin/mold -run cargo build --tests --lib --bin vector \
        --no-default-features --features $FEATURES && \
        cp target/debug/vector /usr/bin/vector; \
    fi
