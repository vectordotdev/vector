ARG RUST_VERSION=1.85
ARG FEATURES

FROM docker.io/rust:${RUST_VERSION}-slim-trixie

RUN apt-get update && apt-get -y --no-install-recommends install \
    build-essential \
    cmake \
    curl \
    git \
    clang \
    libclang-dev \
    libsasl2-dev \
    libssl-dev \
    libxxhash-dev \
    unzip \
    zlib1g-dev \
    zlib1g \
    mold

COPY tests/data/ca/certs /certs

COPY scripts/environment/install-protoc.sh /
COPY scripts/environment/prepare.sh /
COPY scripts/environment/binstall.sh /

RUN bash /prepare.sh --modules=cargo-nextest
RUN bash /install-protoc.sh

WORKDIR /vector
COPY . .
ARG FEATURES

RUN --mount=type=cache,target=/vector/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    /usr/bin/mold -run cargo build --tests --lib --bin vector \
    --no-default-features --features $FEATURES && \
    cp target/debug/vector /usr/bin/vector
