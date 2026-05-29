# syntax=docker/dockerfile:1
#
# Multi-stage build for the Antithesis disk-buffer-v2 harness.
#   target `vector`   : minimal-feature release Vector (the SUT)
#   target `workload` : instrumented Rust workload driver (the client)
#
# Build context is the repo root (see antithesis/config/docker-compose.yaml).

############################
# Vector SUT — build stage #
############################
FROM rust:1.92-bookworm AS vector-build
# System deps Vector's build commonly needs (protoc for prometheus/prost, cmake,
# perl/ssl for vendored crates).
RUN apt-get update && apt-get install -y --no-install-recommends \
        protobuf-compiler cmake perl build-essential pkg-config libssl-dev clang \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
# Copy ONLY Vector build inputs (not antithesis/) so harness/config edits don't
# invalidate this (expensive) compile layer. Mirrors the .dockerignore allowlist
# minus antithesis/.
COPY Cargo.toml Cargo.lock rust-toolchain.toml build.rs ./
COPY lib ./lib
COPY src ./src
COPY proto ./proto
COPY benches ./benches
COPY tests ./tests
COPY vdev ./vdev
COPY scripts ./scripts
# Minimal feature set: just the sources/sinks the harness needs. Disk buffer
# support is core (always compiled). Cache the registry + target dirs across
# rebuilds; copy the binary out of the cache mount within the same RUN.
# Build with Antithesis LLVM coverage instrumentation (sancov). The flags are
# applied via --config to the TARGET only, so host build-scripts/proc-macros are
# not instrumented. The antithesis-instrumentation crate (dep of vector-buffers)
# provides the libvoidstar loader + sancov callbacks; build-id is required for
# symbolization.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --no-default-features \
      --features "sources-http_server,sinks-http,sources-internal_metrics,sinks-prometheus,sources-demo_logs" \
      --bin vector \
      --config 'build.target = "x86_64-unknown-linux-gnu"' \
      --config 'target.x86_64-unknown-linux-gnu.rustflags = ["-Cpasses=sancov-module","-Cllvm-args=-sanitizer-coverage-level=3","-Cllvm-args=-sanitizer-coverage-trace-pc-guard","-Clink-args=-Wl,--build-id"]' \
    && cp target/x86_64-unknown-linux-gnu/release/vector /usr/local/bin/vector

#############################
# Vector SUT — runtime stage #
#############################
FROM debian:stable-slim AS vector
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=vector-build /usr/local/bin/vector /usr/bin/vector
COPY tests/antithesis/config/vector.yaml /etc/vector/vector.yaml
RUN mkdir -p /var/lib/vector
# Symbolization: expose the instrumented (unstripped, build-id'd) binary.
RUN mkdir -p /symbols && ln -s /usr/bin/vector /symbols/vector
ENV NO_COLOR=1
EXPOSE 8080 9598
# Vector is now built WITH sancov coverage instrumentation (antithesis-
# instrumentation crate + sancov RUSTFLAGS) and a SUT-side assert_always at the
# ledger total_buffer_size underflow site (lib/vector-buffers) — the precise
# #21683 signal. Assertions/instrumentation are no-ops outside Antithesis.
ENTRYPOINT ["/usr/bin/vector", "--config", "/etc/vector/vector.yaml"]

##############################
# Workload — build stage     #
##############################
FROM rust:1.92-bookworm AS workload-build
WORKDIR /w
COPY tests/antithesis/workload/Cargo.toml ./Cargo.toml
COPY tests/antithesis/workload/src ./src
# LLVM coverage instrumentation per the Antithesis Rust guide. build-id is
# required for symbolization.
ENV RUSTFLAGS="-Ccodegen-units=1 -Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=3 -Cllvm-args=-sanitizer-coverage-trace-pc-guard -Clink-args=-Wl,--build-id"
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --target x86_64-unknown-linux-gnu \
    && cp target/x86_64-unknown-linux-gnu/release/vdbuf-workload /usr/local/bin/vdbuf-workload

##############################
# Workload — runtime stage   #
##############################
FROM debian:stable-slim AS workload
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=workload-build /usr/local/bin/vdbuf-workload /usr/bin/vdbuf-workload
# Symbolization: expose the unstripped (DWARF) binary under /symbols.
RUN mkdir -p /symbols && ln -s /usr/bin/vdbuf-workload /symbols/vdbuf-workload
COPY tests/antithesis/test/v1 /opt/antithesis/test/v1
COPY tests/antithesis/setup-complete.sh /usr/bin/setup-complete.sh
ENV NO_COLOR=1
ENV VECTOR_SOURCE_URL=http://vdbuf-vector:8080/ \
    VECTOR_METRICS_URL=http://vdbuf-vector:9598/metrics \
    COLLECTOR_ADDR=0.0.0.0:8686
ENTRYPOINT ["/usr/bin/vdbuf-workload", "serve"]
