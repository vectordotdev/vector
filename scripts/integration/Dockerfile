ARG RUST_VERSION
FROM docker.io/rust:${RUST_VERSION}-slim-bookworm

COPY scripts/cross/bootstrap-ubuntu.sh scripts/environment/install-protoc.sh /
COPY tests/data/ca/certs /certs
RUN /bootstrap-ubuntu.sh
