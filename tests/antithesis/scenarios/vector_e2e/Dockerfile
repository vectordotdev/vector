# syntax=docker/dockerfile:1

ARG SANCOV_RUSTFLAGS='["-Cpasses=sancov-module","-Cllvm-args=-sanitizer-coverage-level=3","-Cllvm-args=-sanitizer-coverage-trace-pc-guard","-Clink-args=-Wl,--build-id"]'

############################
# Vector SUT — build stage #
############################
FROM rust:1.92-bookworm AS vector-build
RUN apt-get update && apt-get install -y --no-install-recommends \
        protobuf-compiler cmake perl build-essential pkg-config libssl-dev clang binutils \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml build.rs ./
COPY lib ./lib
COPY src ./src
COPY proto ./proto
COPY benches ./benches
COPY tests ./tests
COPY vdev ./vdev
COPY scripts ./scripts
ARG SANCOV_RUSTFLAGS
# debug=true keeps DWARF for /symbols. lto=false keeps sancov instrumentation
# predictable and stops the optimizer from dropping the force-linked runtime.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --no-default-features \
      --features "sources-http_server,sinks-http,sources-internal_metrics,sinks-prometheus,antithesis-scenario-memory" \
      --bin vector \
      --config 'build.target = "x86_64-unknown-linux-gnu"' \
      --config 'profile.release.debug = true' \
      --config 'profile.release.lto = false' \
      --config "target.x86_64-unknown-linux-gnu.rustflags = ${SANCOV_RUSTFLAGS}" \
    && cp target/x86_64-unknown-linux-gnu/release/vector /usr/local/bin/vector \
    && echo "validating instrumentation symbols..." \
    && nm /usr/local/bin/vector | grep -q __sanitizer_cov_trace_pc_guard \
    && nm /usr/local/bin/vector | grep -q antithesis_load_libvoidstar \
    && echo "instrumentation OK"

##################################
# Harness (workload) — build stage
##################################
# The workload binaries live in the shared `harness` crate, a member of Vector's
# workspace, so the build needs the workspace root and member manifests. `-p`
# compiles only the harness bins.
FROM rust:1.92-bookworm AS workload-build
RUN apt-get update && apt-get install -y --no-install-recommends binutils \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml build.rs ./
COPY lib ./lib
COPY src ./src
COPY proto ./proto
COPY benches ./benches
COPY tests ./tests
COPY vdev ./vdev
COPY scripts ./scripts
ARG SANCOV_RUSTFLAGS
# debug=true keeps DWARF for /symbols.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p antithesis-harness \
      --config 'build.target = "x86_64-unknown-linux-gnu"' \
      --config 'profile.release.debug = true' \
      --config "target.x86_64-unknown-linux-gnu.rustflags = ${SANCOV_RUSTFLAGS}" \
    && D=target/x86_64-unknown-linux-gnu/release \
    && cp "$D/oracle" "$D/parallel_driver_produce" "$D/eventually_conservation" /usr/local/bin/ \
    && echo "validating instrumentation symbols..." \
    && nm /usr/local/bin/oracle | grep -q __sanitizer_cov_trace_pc_guard \
    && nm /usr/local/bin/oracle | grep -q antithesis_load_libvoidstar \
    && echo "instrumentation OK"

#####################################
# Runtime: Vector SUT (the one node) #
#####################################
FROM debian:stable-slim AS vector
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=vector-build /usr/local/bin/vector /usr/bin/vector
# Bake the node config; compose points --config at it.
COPY tests/antithesis/scenarios/vector_e2e/vector.yaml /etc/vector/vector.yaml
RUN mkdir -p /symbols && ln -s /usr/bin/vector /symbols/vector
ENV NO_COLOR=1
EXPOSE 8080 9598
ENTRYPOINT ["/usr/bin/vector"]

###################################
# Runtime: workload (oracle + cmds) #
###################################
FROM debian:stable-slim AS workload
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# The oracle is the entrypoint. The two test-command binaries are the test commands:
# drop them straight into the test template, named by their Antithesis prefix.
COPY --from=workload-build /usr/local/bin/oracle /usr/bin/oracle
COPY --from=workload-build /usr/local/bin/parallel_driver_produce /opt/antithesis/test/v1/ve2e/parallel_driver_produce
COPY --from=workload-build /usr/local/bin/eventually_conservation /opt/antithesis/test/v1/ve2e/eventually_conservation
# Symbolize all three instrumented binaries: the oracle entrypoint and both
# test-command bins. A crash in any of them must resolve against /symbols.
RUN mkdir -p /symbols \
 && ln -s /usr/bin/oracle /symbols/oracle \
 && ln -s /opt/antithesis/test/v1/ve2e/parallel_driver_produce /symbols/parallel_driver_produce \
 && ln -s /opt/antithesis/test/v1/ve2e/eventually_conservation /symbols/eventually_conservation
ENV NO_COLOR=1
# The oracle waits for the SUT, emits setup_complete via the SDK, then serves so
# Antithesis runs the test commands in this container.
ENTRYPOINT ["/usr/bin/oracle"]
