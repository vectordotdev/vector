ARG RUST_VERSION
ARG DEBIAN_FLAVOR=bullseye
FROM rust:${RUST_VERSION}-${DEBIAN_FLAVOR} AS builder

RUN --mount=type=cache,id=apt,target=/var/cache/apt \
  apt-get update \
  && apt-get install -y cmake clang-11 libsasl2-dev \
  && rm -rf /var/lib/apt/lists/*

COPY . /code

WORKDIR /code

RUN --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
  --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
  cargo build --release

ARG DEBIAN_FLAVOR=bullseye-slim
FROM debian:${DEBIAN_FLAVOR}

RUN apt-get update && apt-get install -y ca-certificates tzdata systemd && rm -rf /var/lib/apt/lists/*

COPY --from=builder /code/target/release/vector /usr/bin/vector

VOLUME /var/lib/vector/

ENTRYPOINT ["/usr/bin/vector"]