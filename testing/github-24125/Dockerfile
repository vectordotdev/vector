# syntax=docker/dockerfile:1.4
#
# Minimal Vector image used by reproduce-reload-deadlock.sh when no image is
# supplied. Builds for the host architecture with only the features the repro
# needs (no cross-compile, no Rosetta). BuildKit cache mounts preserve the
# cargo registry and target dir across rebuilds.

FROM rust:1.92-bookworm AS build
WORKDIR /vector
RUN apt-get update && apt-get install -y --no-install-recommends \
      cmake protobuf-compiler libsasl2-dev pkg-config clang libclang-dev && \
    rm -rf /var/lib/apt/lists/*
COPY . .
ARG FEATURES="api,api-client,sources-demo_logs,sources-internal_metrics,transforms-route,sinks-http,sinks-prometheus"
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/vector/target \
    cargo build --release --bin vector --no-default-features --features "$FEATURES" && \
    cp /vector/target/release/vector /vector/vector

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates && \
    rm -rf /var/lib/apt/lists/*
COPY --from=build /vector/vector /usr/local/bin/vector
ENTRYPOINT ["vector"]
CMD ["--config-dir", "/etc/vector"]
