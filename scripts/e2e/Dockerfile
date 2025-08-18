ARG RUST_VERSION=1.85
ARG FEATURES

FROM docker.io/rust:${RUST_VERSION}-slim-bookworm

RUN apt-get update && apt-get -y --no-install-recommends install \
    build-essential \
    cmake \
    curl \
    git \
    clang \
    libclang-dev \
    libsasl2-dev \
    libstdc++-11-dev \
    libssl-dev \
    libxxhash-dev \
    unzip \
    zlib1g-dev \
    zlib1g \
    mold

RUN cargo install cargo-nextest --version 0.9.95 --locked

COPY scripts/environment/install-protoc.sh /
COPY tests/data/ca/certs /certs
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
