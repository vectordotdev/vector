# syntax=docker/dockerfile:1
#
# Antithesis images for the `vector_to_vector_e2e_disk` experiment.
# Build context is the repo root. Two runtime targets, selected by the scenario's
# docker-compose.yaml `target:`.
#
# Build stages (compiled with Antithesis sancov coverage; the sancov RUSTFLAGS
# are applied to the TARGET only via --config, so host build-scripts/proc-macros
# are not instrumented and link cleanly). Both stages assert post-build that the
# instrumentation symbols are present — the saluki safety net that would have
# caught a dropped force-link.
#   vector-build   : full Vector with disk buffer v2 (the chained SUT)
#   harness-build  : the workload test-command bins + collector
#
# Runtime targets:
#   vector    : the SUT; bakes node0/1/2.yaml (compose picks one per node)
#   workload  : the collector entrypoint + test-command bins + test template

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
# debug=true keeps DWARF for /symbols; lto=false keeps sancov instrumentation
# predictable and stops the optimizer from dropping the force-linked runtime.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --no-default-features \
      --features "sources-http_server,sinks-http,sources-internal_metrics,sinks-prometheus,sources-demo_logs,sources-vector,sinks-vector,antithesis" \
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
# The harness is its own small workspace (HTTP only — no vector-buffers), so it
# needs only its own dir.
FROM rust:1.92-bookworm AS harness-build
RUN apt-get update && apt-get install -y --no-install-recommends binutils \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /h
COPY tests/antithesis/harness/ ./
ARG SANCOV_RUSTFLAGS
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/h/target \
    cargo build --release \
      --config 'build.target = "x86_64-unknown-linux-gnu"' \
      --config "target.x86_64-unknown-linux-gnu.rustflags = ${SANCOV_RUSTFLAGS}" \
    && D=target/x86_64-unknown-linux-gnu/release \
    && cp "$D/collector" "$D/parallel_driver_produce" "$D/eventually_conservation" /usr/local/bin/ \
    && echo "validating instrumentation symbols..." \
    && nm /usr/local/bin/collector | grep -q __sanitizer_cov_trace_pc_guard \
    && nm /usr/local/bin/collector | grep -q antithesis_load_libvoidstar \
    && echo "instrumentation OK"

#####################################
# Runtime: Vector SUT (one per node) #
#####################################
FROM debian:stable-slim AS vector
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=vector-build /usr/local/bin/vector /usr/bin/vector
# Bake the node configs (compose picks one per node via --config) plus node0's
# benign alternate, which the reload fault swaps in to force a sink rebuild.
COPY tests/antithesis/scenarios/vector_to_vector_e2e_disk/node0.yaml /etc/vector/node0.yaml
COPY tests/antithesis/scenarios/vector_to_vector_e2e_disk/node0.b.yaml /etc/vector/node0.b.yaml
COPY tests/antithesis/scenarios/vector_to_vector_e2e_disk/node1.yaml /etc/vector/node1.yaml
# The reload fault is an anytime_ test command that runs IN the node container.
# It is a no-op on node1 (no VECTOR_CONFIG_ALT). The node stays running because
# its entrypoint is Vector, not a test command.
COPY --chmod=755 tests/antithesis/scenarios/vector_to_vector_e2e_disk/anytime_reload.sh /opt/antithesis/test/v1/v2v/anytime_reload
RUN mkdir -p /var/lib/vector /symbols && ln -s /usr/bin/vector /symbols/vector
ENV NO_COLOR=1
EXPOSE 6000 8080 9598
ENTRYPOINT ["/usr/bin/vector"]

#######################################
# Runtime: workload (collector + cmds) #
#######################################
FROM debian:stable-slim AS workload
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# The collector is the entrypoint. The two test-command binaries ARE the test
# commands: drop them straight into the test template, named by their Antithesis
# prefix. No shell wrappers.
COPY --from=harness-build /usr/local/bin/collector /usr/bin/collector
COPY --from=harness-build /usr/local/bin/parallel_driver_produce /opt/antithesis/test/v1/v2v/parallel_driver_produce
COPY --from=harness-build /usr/local/bin/eventually_conservation /opt/antithesis/test/v1/v2v/eventually_conservation
# Symbolize all three instrumented binaries: the collector entrypoint and both
# test-command bins. A crash in any of them must resolve against /symbols.
RUN mkdir -p /symbols \
 && ln -s /usr/bin/collector /symbols/collector \
 && ln -s /opt/antithesis/test/v1/v2v/parallel_driver_produce /symbols/parallel_driver_produce \
 && ln -s /opt/antithesis/test/v1/v2v/eventually_conservation /symbols/eventually_conservation
ENV NO_COLOR=1
# The collector receives deliveries, waits for the SUT, emits setup_complete via
# the SDK, then idles so Antithesis runs the test commands in this container.
ENTRYPOINT ["/usr/bin/collector"]
