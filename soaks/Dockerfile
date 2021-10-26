#
# BUILDER
#
FROM docker.io/rust:1.56-buster as builder
WORKDIR vector
ARG VECTOR_FEATURES
RUN apt-get -y update && apt-get -y install build-essential cmake libclang-dev libsasl2-dev
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
