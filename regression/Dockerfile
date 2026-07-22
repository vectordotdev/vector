#
# VECTOR BUILDER
#
ARG RUST_VERSION=1
FROM docker.io/rust:${RUST_VERSION}-slim-trixie AS builder

# Copy the whole scripts/environment/ tree so prepare.sh can source its sibling
# scripts on older checkouts in regression baseline builds.
COPY scripts/environment/ /scripts/environment/

RUN bash /scripts/environment/install-debian-build-deps.sh

WORKDIR /vector
COPY rust-toolchain.toml .

RUN bash /scripts/environment/prepare.sh --modules=rustup
RUN bash /scripts/environment/install-protoc.sh

COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/vector/target \
    /usr/bin/mold -run cargo build --bin vector --release && \
    cp target/release/vector .

#
# TARGET
#
FROM docker.io/debian:trixie-slim@sha256:c85a2732e97694ea77237c61304b3bb410e0e961dd6ee945997a06c788c545bb
RUN apt-get update && apt-get dist-upgrade -y && apt-get -y --no-install-recommends install zlib1g ca-certificates libsasl2-2 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /vector/vector /usr/bin/vector
RUN mkdir --parents --mode=0777 /var/lib/vector

# Smoke test
RUN ["/usr/bin/vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
