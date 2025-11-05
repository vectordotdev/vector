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
# SECTION 4: Precompile tests (only in PRECOMPILED MODE)
# ============================================================================
COPY tests/e2e/precompile.sh /precompile.sh
RUN chmod +x /precompile.sh

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    /precompile.sh "$PRECOMPILE" "$FEATURES"

# ============================================================================
# SECTION 5: Aggressive cleanup to reduce image size
# ============================================================================
RUN if [ "$PRECOMPILE" = "true" ]; then \
      echo "==> Cleaning up source and build artifacts to reduce image size..."; \
      rm -rf /home/target; \
      rm -rf /home/vector/src; \
      rm -rf /home/vector/.git; \
      rm -rf /home/vector/benches; \
      rm -rf /home/vector/distribution; \
      rm -rf /home/vector/lib; \
      rm -rf /home/vector/scripts; \
      rm -rf /home/vector/website; \
      find /home/vector -name "*.rs" -delete; \
      echo "==> Cleanup complete"; \
    fi

# ============================================================================
# SECTION 6: Setup entrypoint for volume seeding
# ============================================================================
COPY tests/e2e/seed-target.sh /seed-target.sh
RUN chmod +x /seed-target.sh

ENTRYPOINT ["/seed-target.sh"]
CMD ["/bin/sleep", "infinity"]
