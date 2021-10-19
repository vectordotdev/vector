ARG FEATURES
ARG RUST_VERSION=1.55
ARG ALPINE_VERSION=3.14

# load dependencies once, for all platforms
FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-alpine${ALPINE_VERSION} AS vendor

ENV USER=root
WORKDIR /code
COPY . /code
RUN mkdir -p /code/.cargo && cargo vendor >> /code/.cargo/config

# build package for dedicated platform
FROM rust:${RUST_VERSION}-alpine${ALPINE_VERSION} AS builder
ARG FEATURES

# RUN apt-get update \
#   && apt-get install -y cmake libclang1-9 llvm-9 libsasl2-dev
RUN apk add --no-cache build-base cmake libc-dev libsasl protoc

COPY --from=vendor /code /code

WORKDIR /code

RUN cargo build --release --offline ${FEATURES}
