# ============================================================================
# Universal Test Runner Image for Vector
# ============================================================================
#
# This Dockerfile supports TWO MODES of operation:
#
# 1. PRECOMPILED MODE (PRECOMPILE=true, for CI):
#    - Compiles all integration + e2e tests during image build
#    - Tests run instantly from precompiled binaries
#    - Use with: cargo vdev int build (to build image)
#                cargo vdev int test --reuse-image <test> (to run tests)
#                cargo vdev e2e test --reuse-image <test> (to run tests)
#
# 2. DEVELOPMENT MODE (PRECOMPILE=false, for local iteration):
#    - Skips precompilation during image build
#    - Source code mounted at runtime for live editing
#    - Tests compile on-demand when you run them
#    - Use with: cargo vdev int test <test> (no --reuse-image flag)
#
# ============================================================================

# Build arguments
ARG RUST_VERSION=1
ARG FEATURES
ARG PRECOMPILE=false

# Base image
FROM docker.io/rust:${RUST_VERSION}-slim-trixie

# ============================================================================
# SECTION 1: Install system dependencies
# ============================================================================
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

# ============================================================================
# SECTION 2: Install certificates and build tools
# ============================================================================
COPY tests/data/ca/certs /certs
COPY scripts/environment/install-protoc.sh /
COPY scripts/environment/prepare.sh /
COPY scripts/environment/binstall.sh /
COPY scripts/environment/release-flags.sh /

RUN bash /prepare.sh --modules=cargo-nextest
RUN bash /install-protoc.sh

# ============================================================================
# SECTION 3: Copy source code
# ============================================================================
ARG FEATURES
ARG PRECOMPILE

WORKDIR /home/vector
COPY . .

# ============================================================================
# SECTION 4: Compile tests (only in PRECOMPILED MODE)
# ============================================================================
# Compiles tests with FEATURES when PRECOMPILE=true
# Stores at /precompiled-target/debug for volume seeding
# ============================================================================
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    if [ "$PRECOMPILE" = "true" ]; then \
        echo "==> Compiling tests with features: $FEATURES"; \
        CARGO_BUILD_TARGET_DIR=/home/target \
        /usr/bin/mold -run cargo build \
            --tests \
            --lib \
            --bin vector \
            --no-default-features \
            --features $FEATURES && \
        echo "==> Installing vector binary to /usr/bin/vector" && \
        cp /home/target/debug/vector /usr/bin/vector && \
        echo "==> Copying binaries to /precompiled-target for volume seeding" && \
        mkdir -p /precompiled-target && \
        cp -r /home/target/debug /precompiled-target/; \
    else \
        echo "==> Skipping test precompilation (PRECOMPILE=false)"; \
    fi

# ============================================================================
# SECTION 5: Setup entrypoint for volume seeding
# ============================================================================
COPY tests/e2e/seed-target.sh /seed-target.sh
RUN chmod +x /seed-target.sh

ENTRYPOINT ["/seed-target.sh"]
CMD ["/bin/sleep", "infinity"]
