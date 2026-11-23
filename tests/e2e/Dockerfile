ARG RUST_VERSION=1

FROM docker.io/rust:${RUST_VERSION}-slim-trixie

COPY scripts/environment/install-debian-build-deps.sh /
RUN bash /install-debian-build-deps.sh

# Provide `envsubst` (from gettext-base) so e2e compose services can render
# config templates with environment-variable substitution at container startup.
RUN apt-get update && apt-get install -y --no-install-recommends gettext-base && rm -rf /var/lib/apt/lists/*

COPY tests/data/ca/certs /certs

COPY scripts/environment/install-protoc.sh /
COPY scripts/environment/prepare.sh /
COPY scripts/environment/binstall.sh /

WORKDIR /vector
COPY rust-toolchain.toml .

RUN bash /prepare.sh --modules=cargo-nextest,cargo-llvm-cov
RUN bash /install-protoc.sh
