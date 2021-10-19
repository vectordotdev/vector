ARG FEATURES
ARG RUST_VERSION=1.55
ARG DEBIAN_VERSION=bullseye

# load dependencies once, for all platforms
FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS vendor

ENV USER=root
WORKDIR /code
COPY . /code
RUN mkdir -p /code/.cargo && cargo vendor >> /code/.cargo/config

# build package for dedicated platform
FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS builder
ARG FEATURES

RUN apt-get update \
  && apt-get install -y cmake libclang1-9 llvm-9 libsasl2-dev

COPY --from=vendor /code /code

WORKDIR /code

RUN cargo build --release --offline ${FEATURES}

FROM debian:${DEBIAN_VERSION} AS image

COPY --from=builder /code/target/release/vector /usr/bin/vector

ENTRYPOINT ["/usr/bin/vector"]
CMD ["--help"]
