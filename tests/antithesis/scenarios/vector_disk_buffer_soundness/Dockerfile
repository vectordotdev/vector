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
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --no-default-features \
      --features "sources-http_server,sinks-http,sources-internal_metrics,sinks-prometheus,antithesis-scenario-disk" \
      --bin vector \
      --config 'build.target = "x86_64-unknown-linux-gnu"' \
      --config 'profile.release.debug = true' \
      --config 'profile.release.lto = false' \
      --config "target.x86_64-unknown-linux-gnu.rustflags = ${SANCOV_RUSTFLAGS}" \
    && cp target/x86_64-unknown-linux-gnu/release/vector /usr/local/bin/vector \
    && nm /usr/local/bin/vector | grep -q __sanitizer_cov_trace_pc_guard \
    && nm /usr/local/bin/vector | grep -q antithesis_load_libvoidstar

##################################
# Harness (workload) — build stage
##################################
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
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release -p antithesis-harness \
      --config 'build.target = "x86_64-unknown-linux-gnu"' \
      --config 'profile.release.debug = true' \
      --config "target.x86_64-unknown-linux-gnu.rustflags = ${SANCOV_RUSTFLAGS}" \
    && D=target/x86_64-unknown-linux-gnu/release \
    && cp "$D/disk_soundness_oracle" "$D/parallel_driver_disk_soundness" \
          "$D/eventually_disk_soundness" /usr/local/bin/ \
    && nm /usr/local/bin/disk_soundness_oracle | grep -q __sanitizer_cov_trace_pc_guard

#######################
# Runtime: Vector SUT #
#######################
FROM debian:stable-slim AS vector
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=vector-build /usr/local/bin/vector /usr/bin/vector
COPY tests/antithesis/scenarios/vector_disk_buffer_soundness/vector.yaml /etc/vector/vector.yaml
RUN mkdir -p /var/lib/vector /symbols \
 && ln -s /usr/bin/vector /symbols/vector
ENV NO_COLOR=1
EXPOSE 8080 9598
ENTRYPOINT ["/usr/bin/vector", "--config", "/etc/vector/vector.yaml"]

###################
# Runtime: oracle #
###################
FROM debian:stable-slim AS oracle
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=workload-build /usr/local/bin/disk_soundness_oracle /usr/bin/disk_soundness_oracle
RUN mkdir -p /symbols \
 && ln -s /usr/bin/disk_soundness_oracle /symbols/disk_soundness_oracle
ENV NO_COLOR=1
EXPOSE 8686
ENTRYPOINT ["/usr/bin/disk_soundness_oracle"]

##########################
# Runtime: test commands #
##########################
FROM debian:stable-slim AS workload
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=workload-build /usr/local/bin/parallel_driver_disk_soundness /opt/antithesis/test/v1/disk/parallel_driver_disk_soundness
COPY --from=workload-build /usr/local/bin/eventually_disk_soundness /opt/antithesis/test/v1/disk/eventually_disk_soundness
RUN mkdir -p /symbols \
 && ln -s /opt/antithesis/test/v1/disk/parallel_driver_disk_soundness /symbols/parallel_driver_disk_soundness \
 && ln -s /opt/antithesis/test/v1/disk/eventually_disk_soundness /symbols/eventually_disk_soundness
ENV NO_COLOR=1
ENTRYPOINT ["sleep", "infinity"]
