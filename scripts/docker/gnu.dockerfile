ARG FEATURES
ARG RUST_VERSION=1.57
ARG DEBIAN_VERSION=bullseye

# load dependencies once, for all platforms
FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS vendor

ENV USER=root
WORKDIR /code
COPY Cargo.toml Cargo.lock /code/
COPY src /code/src
COPY lib /code/lib
RUN mkdir -p /code/.cargo && cargo vendor >> /code/.cargo/config

# build package for dedicated platform
FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS builder-base
ARG FEATURES

RUN --mount=type=cache,id=aptlist,target=/var/lib/apt/lists \
  apt-get update \
  && apt-get install -y cmake dpkg dpkg-dev liblzma-dev libclang1-9 llvm-9 libsasl2-dev protobuf-compiler

COPY --from=vendor /code /code
COPY build.rs /code/build.rs
COPY proto /code/proto
COPY benches /code/benches

WORKDIR /code

FROM builder-base AS builder-bin

ARG FEATURES
RUN cargo build --release -j $(($(nproc) /2)) --offline ${FEATURES}

FROM builder-bin AS builder-deb

RUN --mount=type=cache,id=aptlist,target=/var/lib/apt/lists \
  apt-get update \
  && apt-get install -y cmark-gfm dpkg dpkg-dev liblzma-dev

RUN cargo install cargo-deb

COPY README.md LICENSE NOTICE /code/
COPY config /code/config
COPY distribution /code/distribution
COPY scripts /code/scripts

RUN bash ./scripts/package-deb.sh

FROM scratch AS deb

COPY --from=builder-deb /code/target/deb/* .

FROM debian:${DEBIAN_VERSION} AS image

COPY --from=builder-bin /code/target/release/vector /usr/bin/vector

ENTRYPOINT ["/usr/bin/vector"]
CMD ["--help"]
