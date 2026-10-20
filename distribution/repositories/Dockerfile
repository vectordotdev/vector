FROM rust:1.96.1-bookworm@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663 AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --locked --package vdev --features package-repositories \
    && strip target/debug/vdev

FROM ubuntu:24.04@sha256:68ee22004a9109b44973313b02ff7b1de250f1de0f5ecb628d73f579aaff33ac
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    apt-utils ca-certificates createrepo-c dpkg-dev gnupg rpm libssl3t64 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/debug/vdev /usr/local/bin/vdev
# vdev normally locates its checkout before dispatching a command.
COPY --from=build /src/Cargo.toml /app/Cargo.toml
WORKDIR /app
ENTRYPOINT ["vdev", "release", "repositories"]
