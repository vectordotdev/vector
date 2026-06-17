ARG RUST_VERSION=1
ARG FEATURES
ARG BUILD=false

FROM docker.io/rust:${RUST_VERSION}-slim-trixie AS base

COPY scripts/environment/install-debian-build-deps.sh /
RUN bash /install-debian-build-deps.sh

COPY tests/data/ca/certs /certs

COPY scripts/environment/install-protoc.sh /
COPY scripts/environment/prepare.sh /
COPY scripts/environment/binstall.sh /

WORKDIR /vector
COPY rust-toolchain.toml .

RUN bash /prepare.sh --modules=cargo-nextest,cargo-llvm-cov
RUN bash /install-protoc.sh

FROM base AS build-false

FROM base AS build-true
COPY . .
ARG FEATURES
RUN --mount=type=cache,target=/vector/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    /usr/bin/mold -run cargo build --tests --lib --bin vector \
    --no-default-features --features $FEATURES && \
    cp target/debug/vector /usr/bin/vector

FROM build-${BUILD}
