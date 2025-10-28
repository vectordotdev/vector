ARG RUST_VERSION=1
ARG FEATURES

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

RUN bash /prepare.sh --modules=cargo-nextest
RUN bash /install-protoc.sh

WORKDIR /vector
COPY . .
ARG FEATURES
ARG TARGET_PATH=/home/target

# Build tests into TARGET_PATH so they persist in the image
# Docker will initialize the vector_target volume from TARGET_PATH on first container run
ENV CARGO_BUILD_TARGET_DIR=${TARGET_PATH}

# Configure cargo to use mold linker for faster builds and linking via environment variables
# This avoids TOML section duplication issues
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold -C link-args=-rdynamic"
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=clang
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold -C link-args=-rdynamic"

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --tests --lib --bin vector \
    --no-default-features --features $FEATURES && \
    cp ${CARGO_BUILD_TARGET_DIR}/debug/vector /usr/bin/vector
