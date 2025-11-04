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

RUN bash /prepare.sh --modules=cargo-nextest
RUN bash /install-protoc.sh

ARG FEATURES
ARG BUILD

# Copy source to /home/vector (same location used at runtime)
WORKDIR /home/vector
COPY . .

# When BUILD=true, compile and persist test binaries in the image.
# The universal test image (BUILD=true) contains:
# - All integration test binaries (all-integration-tests feature)
# - All e2e test binaries (all-e2e-tests feature)
# - The vector binary used as a service by tests
#
# Build target dir is set to /home/target to match runtime configuration.
# Binaries are also copied to /precompiled-target so we can seed the volume at container start.
#
# When BUILD=false, skip precompilation (tests compile at runtime using cache mounts).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    if [ "$BUILD" = "true" ]; then \
        CARGO_BUILD_TARGET_DIR=/home/target /usr/bin/mold -run cargo build --tests --lib --bin vector \
        --no-default-features --features $FEATURES && \
        cp /home/target/debug/vector /usr/bin/vector && \
        mkdir -p /precompiled-target && \
        cp -r /home/target/debug /precompiled-target/; \
    fi

# Copy the seed script that populates /home/target volume with precompiled binaries
COPY tests/e2e/seed-target.sh /seed-target.sh
RUN chmod +x /seed-target.sh

ENTRYPOINT ["/seed-target.sh"]
CMD ["/bin/sleep", "infinity"]
