ARG RUST_VERSION=1.56
ARG DEBIAN_VERSION=bullseye

#
# BUILDER
#
FROM docker.io/rust:${RUST_VERSION}-${DEBIAN_VERSION} as builder
RUN apt-get -y update && apt-get -y install build-essential cmake libclang-dev libsasl2-dev
WORKDIR vector
ARG VECTOR_FEATURES
COPY . .
RUN cargo build --release --bin vector --no-default-features --features "${VECTOR_FEATURES}"

#
# TARGET
#
FROM gcr.io/distroless/cc-debian11
COPY --from=builder /vector/target/release/vector /usr/bin/vector
VOLUME /var/lib/vector/

# Smoke test
RUN ["vector", "--version"]

ENTRYPOINT ["/usr/bin/vector"]
